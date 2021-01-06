use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::{
    task::{Context, Poll},
    time::Duration,
};

use exocore_core::cell::Cell;
use exocore_core::futures::{interval, spawn_blocking, BatchingStream};
use exocore_core::protos::core::NodeStoreConfig;
use exocore_core::protos::generated::exocore_store::entity_mutation::Mutation;
use exocore_core::protos::generated::exocore_store::{
    entity_query, EntityQuery, EntityResults, MutationRequest, MutationResult,
};
use exocore_core::protos::{prost::ProstMessageExt, store::OperationsPredicate};
use exocore_core::time::Clock;
use exocore_core::utils::handle_set::{Handle, HandleSet};
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;

use super::entity_index::EntityIndex;
use crate::error::Error;
use crate::local::watched_queries::WatchedQueries;
use crate::query::WatchToken;
use crate::{local::mutation_tracker::MutationTracker, mutation::MutationRequestLike};

/// Locally persisted entities store allowing mutation and queries on entities
/// and their traits.
///
/// It forwards mutation requests to the chain engine, receives back chain
/// events that get indexed by the entities index. Queries are executed by the
/// entities index.
pub struct Store<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    config: StoreConfig,
    handle_set: HandleSet,
    incoming_queries_receiver: mpsc::Receiver<QueryRequest>,
    inner: Arc<RwLock<Inner<CS, PS>>>,
}

impl<CS, PS> Store<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    pub fn new(
        config: StoreConfig,
        cell: Cell,
        clock: Clock,
        chain_handle: exocore_chain::engine::EngineHandle<CS, PS>,
        index: EntityIndex<CS, PS>,
    ) -> Result<Store<CS, PS>, Error> {
        let (incoming_queries_sender, incoming_queries_receiver) =
            mpsc::channel::<QueryRequest>(config.query_channel_size);

        let inner = Arc::new(RwLock::new(Inner {
            cell,
            clock,
            index,
            watched_queries: WatchedQueries::new(),
            mutation_tracker: MutationTracker::new(config),
            incoming_queries_sender,
            chain_handle,
        }));

        Ok(Store {
            config,
            handle_set: HandleSet::new(),
            incoming_queries_receiver,
            inner,
        })
    }

    pub fn get_handle(&self) -> StoreHandle<CS, PS> {
        StoreHandle {
            config: self.config,
            handle: self.handle_set.get_handle(),
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub async fn run(self) -> Result<(), Error> {
        let config = self.config;

        // Incoming queries execution
        let incoming_queries_receiver = self.incoming_queries_receiver;
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        let queries_executor = async move {
            let mut executed_queries = incoming_queries_receiver
                .map(move |watch_request: QueryRequest| {
                    let weak_inner = weak_inner1.clone();
                    async move {
                        let result =
                            Inner::execute_query_async(weak_inner, watch_request.query.clone())
                                .await;
                        (result, watch_request)
                    }
                })
                .buffer_unordered(config.query_parallelism);

            while let Some((result, query_request)) = executed_queries.next().await {
                let inner = weak_inner2.upgrade().ok_or(Error::Dropped)?;
                let inner = inner.read()?;

                if let Err(err) = &result {
                    warn!("Error executing query: err={}", err);
                }

                let should_reply = match (&query_request.sender, &result) {
                    (QueryRequestSender::WatchedQuery(_sender, watch_token), Ok(result)) => inner
                        .watched_queries
                        .update_query_results(*watch_token, result),

                    (QueryRequestSender::WatchedQuery(_, watch_token), Err(_err)) => {
                        inner.watched_queries.unwatch_query(*watch_token);
                        true
                    }

                    (QueryRequestSender::Query(_), _result) => true,
                };

                if should_reply {
                    query_request.reply(result);
                }
            }

            Ok::<(), Error>(())
        };

        // Schedules chain engine events stream
        let mut events_stream = {
            let mut inner = self.inner.write()?;
            let events = inner.chain_handle.take_events_stream()?;

            // batching stream consumes all available events from stream without waiting
            BatchingStream::new(events, config.chain_events_batch_size)
        };
        let (mut watch_check_sender, mut watch_check_receiver) = mpsc::channel(1);
        let weak_inner = Arc::downgrade(&self.inner);
        let chain_events_handler = async move {
            while let Some(events) = events_stream.next().await {
                match Inner::handle_chain_engine_events(&weak_inner, events).await {
                    Ok(indexed_operations) if indexed_operations > 0 => {
                        // notify query watching. if it's full, it's guaranteed that it will catch
                        // those changes on next iteration
                        let _ = watch_check_sender.try_send(());
                    }
                    Err(err) => {
                        error!("Error handling chain engine event: {}", err);
                        if err.is_fatal() {
                            return Err(err);
                        }
                    }
                    Ok(_) => {}
                }
            }
            Ok(())
        };

        // Checks if watched queries have their results changed
        let weak_inner = Arc::downgrade(&self.inner);
        let watched_queries_checker = async move {
            while watch_check_receiver.next().await.is_some() {
                let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
                let mut inner = inner.write()?;

                let queries = inner.watched_queries.queries();
                if queries.is_empty() {
                    continue;
                }

                debug!(
                    "Store may have changed because of new events. Checking for {} queries",
                    queries.len()
                );
                for watched_query in queries {
                    let send_result = inner.incoming_queries_sender.try_send(QueryRequest {
                        query: watched_query.query,
                        sender: QueryRequestSender::WatchedQuery(
                            watched_query.sender.clone(),
                            watched_query.token,
                        ),
                    });

                    if let Err(err) = send_result {
                        error!(
                            "Error sending to watched query. Removing it from queries: {}",
                            err
                        );
                        inner.watched_queries.unwatch_query(watched_query.token);
                    }
                }
            }

            Ok::<(), Error>(())
        };

        // Runs a garbage collection pass on entity index, which will only get executed
        // if entities to be collected got flagged in a previous search query and added
        // to the garbage collector queue.
        // See [GarbageCollector](super::entity_index::gc::GarbageCollector)
        let mut gc_interval = interval(config.garbage_collect_interval);
        let weak_inner = Arc::downgrade(&self.inner);
        let garbage_collector = async move {
            while gc_interval.next().await.is_some() {
                let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
                let inner = inner.read()?;
                match inner.index.run_garbage_collector() {
                    Ok(deletions) => {
                        let deletion_request = MutationRequest {
                            mutations: deletions,
                            ..Default::default()
                        };

                        if let Err(err) = inner.handle_mutation_request(deletion_request) {
                            error!("Error executing mutations from garbage collection: {}", err);
                        }
                    }
                    Err(err) => {
                        error!("Error running garbage collection: {}", err);
                        if err.is_fatal() {
                            return Err(err);
                        }
                    }
                }
            }

            Ok::<(), Error>(())
        };

        info!("Entity store started");

        futures::select! {
            _ = self.handle_set.on_handles_dropped().fuse() => {},
            _ = queries_executor.fuse() => {},
            _ = chain_events_handler.fuse() => {},
            _ = watched_queries_checker.fuse() => {},
            _ = garbage_collector.fuse() => {},
        }

        Ok(())
    }
}

/// Configuration for `Store`.
#[derive(Clone, Copy)]
pub struct StoreConfig {
    /// Size of the channel of queries to be executed.
    pub query_channel_size: usize,

    /// Maximum number fo queries to execute in parallel.
    pub query_parallelism: usize,

    /// Size of the result channel of each watched query.
    pub handle_watch_query_channel_size: usize,

    /// Maximum number of events from chain engine to batch together if more are
    /// available.
    pub chain_events_batch_size: usize,

    /// Timeout for mutations that were awaiting for entities to be returned.
    pub mutation_tracker_timeout: Duration,

    /// How often the garbage collection process will run.
    ///
    /// Since garbage collection doesn't happen on the whole index, but only on
    /// entities that got flagged during search, it is better to run more
    /// often than less. `GarbageCollectorConfig::queue_size` can be tweaked
    /// to control rate of collection.
    pub garbage_collect_interval: Duration,
}

impl Default for StoreConfig {
    fn default() -> Self {
        StoreConfig {
            query_channel_size: 1000,
            query_parallelism: 4,
            handle_watch_query_channel_size: 100,
            chain_events_batch_size: 50,
            mutation_tracker_timeout: Duration::from_secs(5),
            garbage_collect_interval: Duration::from_secs(33),
        }
    }
}

impl From<NodeStoreConfig> for StoreConfig {
    fn from(proto: NodeStoreConfig) -> Self {
        let mut config = StoreConfig::default();

        if let Some(v) = proto.query_parallelism {
            config.query_parallelism = v as usize;
        }

        if let Some(index) = &proto.index {
            if let Some(gc) = &index.garbage_collector {
                if let Some(v) = gc.run_interval_secs {
                    config.garbage_collect_interval = Duration::from_secs(v as u64);
                }
            }
        }

        config
    }
}

struct Inner<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    cell: Cell,
    clock: Clock,
    index: EntityIndex<CS, PS>,
    watched_queries: WatchedQueries,
    mutation_tracker: MutationTracker,
    incoming_queries_sender: mpsc::Sender<QueryRequest>,
    chain_handle: exocore_chain::engine::EngineHandle<CS, PS>,
}

impl<CS, PS> Inner<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    fn handle_mutation_request(
        &self,
        mut request: MutationRequest,
    ) -> Result<oneshot::Receiver<Result<MutationResult, Error>>, Error> {
        let (sender, receiver) = oneshot::channel();

        let mut operation_ids = Vec::new();
        let mut last_entity_id = None;
        for mutation in &mut request.mutations {
            if let Some(Mutation::Test(_mutation)) = &mutation.mutation {
                return Err(Error::ProtoFieldExpected("mutation"));
            }

            // if mutation doesn't have an entity id, generate one
            if mutation.entity_id.is_empty() {
                if request.common_entity_id && last_entity_id.is_some() {
                    mutation.entity_id = last_entity_id.unwrap_or_default();
                } else {
                    mutation.entity_id = exocore_core::utils::id::generate_prefixed_id("et");
                }
            }
            last_entity_id = Some(mutation.entity_id.clone());

            // if a put mutation doesn't have a trait id, generate one
            if let Some(Mutation::PutTrait(put_mutation)) = &mut mutation.mutation {
                if let Some(trt) = &mut put_mutation.r#trait {
                    if trt.id.is_empty() {
                        trt.id = exocore_core::utils::id::generate_prefixed_id("trt");
                    }
                }
            }

            let encoded = mutation.encode_to_vec();
            let operation_id = self.chain_handle.write_entry_operation(&encoded)?;

            operation_ids.push(operation_id);
        }

        if (request.wait_indexed || request.return_entities) && !request.mutations.is_empty() {
            self.mutation_tracker.track_request(operation_ids, sender);
        } else {
            let _ = sender.send(Ok(MutationResult {
                operation_ids,
                ..Default::default()
            }));
        }

        Ok(receiver)
    }

    async fn execute_query_async(
        weak_inner: Weak<RwLock<Inner<CS, PS>>>,
        query: Box<EntityQuery>,
    ) -> Result<EntityResults, Error> {
        let result = spawn_blocking(move || {
            let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
            let inner = inner.read()?;

            inner.index.search(query)
        })
        .await;

        result.map_err(|err| Error::Other(format!("Couldn't launch blocking query: {}", err)))?
    }

    async fn handle_chain_engine_events(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        events: Vec<exocore_chain::engine::Event>,
    ) -> Result<usize, Error> {
        let weak_inner = weak_inner.clone();

        let indexed_operations = spawn_blocking(move || -> Result<usize, Error> {
            let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
            let mut inner = inner.write()?;

            let (affected_operations, indexed_operations) =
                inner.index.handle_chain_engine_events(events.into_iter())?;
            inner
                .mutation_tracker
                .handle_indexed_operations(affected_operations.as_slice());

            Ok(indexed_operations)
        })
        .await;

        indexed_operations
            .map_err(|err| Error::Other(format!("Couldn't launch blocking query: {}", err)))?
    }
}

/// Handle to the store, allowing communication to the store asynchronously
pub struct StoreHandle<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    config: StoreConfig,
    handle: Handle,
    inner: Weak<RwLock<Inner<CS, PS>>>,
}

impl<CS, PS> StoreHandle<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    pub async fn on_start(&self) {
        self.handle.on_set_started().await
    }

    #[cfg(test)]
    pub(crate) fn watched_queries(&self) -> Vec<WatchToken> {
        let inner = self.inner.upgrade().unwrap();
        let inner = inner.read().unwrap();

        let mut tokens = Vec::new();
        for query in inner.watched_queries.queries() {
            tokens.push(query.token);
        }

        tokens
    }

    pub async fn mutate<M: Into<MutationRequestLike>>(
        &self,
        request: M,
    ) -> Result<MutationResult, Error> {
        let inner = self.inner.upgrade().ok_or(Error::Dropped)?;

        let request = request.into().0;
        let return_entities = request.return_entities;

        let mutation_future = {
            let inner = inner.read().map_err(|_| Error::Dropped)?;
            inner.handle_mutation_request(request)?
        };

        let mut mutation_result = mutation_future.await.map_err(|_err| Error::Cancelled)??;

        if return_entities {
            let query_result = self
                .query(EntityQuery {
                    predicate: Some(entity_query::Predicate::Operations(OperationsPredicate {
                        operation_ids: mutation_result.operation_ids.clone(),
                    })),
                    ..Default::default()
                })
                .await?;

            mutation_result.entities = query_result
                .entities
                .into_iter()
                .flat_map(|e| e.entity)
                .collect();
        }

        Ok(mutation_result)
    }

    pub async fn query(&self, query: EntityQuery) -> Result<EntityResults, Error> {
        let inner = self.inner.upgrade().ok_or(Error::Dropped)?;

        let receiver = {
            let mut inner = inner.write().map_err(|_| Error::Dropped)?;

            // ok to dismiss send as sender end will be dropped in case of an error and
            // consumer will be notified by channel being closed
            let (sender, receiver) = oneshot::channel();
            let _ = inner.incoming_queries_sender.try_send(QueryRequest {
                query: Box::new(query),
                sender: QueryRequestSender::Query(sender),
            });

            receiver
        };

        receiver.await.map_err(|_err| Error::Cancelled)?
    }

    pub fn watched_query(
        &self,
        mut query: EntityQuery,
    ) -> Result<WatchedQueryStream<CS, PS>, Error> {
        let inner = self.inner.upgrade().ok_or(Error::Dropped)?;
        let mut inner = inner.write().map_err(|_| Error::Dropped)?;

        let mut watch_token = query.watch_token;
        if watch_token == 0 {
            watch_token = inner.clock.consistent_time(inner.cell.local_node()).into();
            query.watch_token = watch_token;
        }

        let (sender, receiver) = mpsc::channel(self.config.handle_watch_query_channel_size);
        let sender = Arc::new(Mutex::new(sender));

        inner
            .watched_queries
            .track_query(watch_token, &query, sender.clone());

        // ok to dismiss send as sender end will be dropped in case of an error and
        // consumer will be notified by channel being closed
        let _ = inner.incoming_queries_sender.try_send(QueryRequest {
            query: Box::new(query),
            sender: QueryRequestSender::WatchedQuery(sender, watch_token),
        });

        Ok(WatchedQueryStream {
            watch_token,
            inner: self.inner.clone(),
            receiver,
        })
    }
}

impl<CS, PS> Clone for StoreHandle<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    fn clone(&self) -> StoreHandle<CS, PS> {
        StoreHandle {
            config: self.config,
            handle: self.handle.clone(),
            inner: self.inner.clone(),
        }
    }
}

/// A query received through the `watched_query` method that needs to be watched
/// and notified when new changes happen to the store that would affects its
/// results.
pub struct WatchedQueryStream<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    watch_token: WatchToken,
    inner: Weak<RwLock<Inner<CS, PS>>>,
    receiver: mpsc::Receiver<Result<EntityResults, Error>>,
}

impl<CS, PS> futures::Stream for WatchedQueryStream<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    type Item = Result<EntityResults, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(cx)
    }
}

impl<CS, PS> Drop for WatchedQueryStream<CS, PS>
where
    CS: exocore_chain::chain::ChainStore,
    PS: exocore_chain::pending::PendingStore,
{
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            if let Ok(inner) = inner.read() {
                inner.watched_queries.unwatch_query(self.watch_token);
            }
        }
    }
}

/// Incoming query to be executed, or re-scheduled watched query to be
/// re-executed.
pub(crate) struct QueryRequest {
    pub query: Box<EntityQuery>,
    pub sender: QueryRequestSender,
}

pub(crate) enum QueryRequestSender {
    Query(oneshot::Sender<Result<EntityResults, Error>>),
    WatchedQuery(
        Arc<Mutex<mpsc::Sender<Result<EntityResults, Error>>>>,
        WatchToken,
    ),
}

impl QueryRequest {
    fn reply(mut self, result: Result<EntityResults, Error>) {
        match self.sender {
            QueryRequestSender::Query(sender) => {
                let _ = sender.send(result);
            }
            QueryRequestSender::WatchedQuery(ref mut sender, _token) => {
                if let Ok(mut sender) = sender.lock() {
                    let _ = sender.try_send(result);
                }
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::time::Duration;

    use exocore_core::protos::prost::ProstAnyPackMessageExt;
    use exocore_core::tests_utils::async_expect_eventually;
    use exocore_core::{futures::sleep, protos::store::Trait, protos::test::TestMessage};
    use futures::executor::block_on_stream;

    use super::super::TestStore;
    use super::*;
    use crate::local::entity_index::GarbageCollectorConfig;
    use crate::local::EntityIndexConfig;
    use crate::mutation::MutationBuilder;
    use crate::query::QueryBuilder;

    #[tokio::test(flavor = "multi_thread")]
    async fn store_mutate_query_via_handle() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let mutation = test_store.create_put_contact_mutation("entry1", "trt1", "Hello World");
        let result = test_store.mutate(mutation).await?;
        assert_eq!(result.entities.len(), 0);
        test_store
            .cluster
            .wait_operation_committed(0, result.operation_ids[0]);

        let query = QueryBuilder::matches("hello").build();
        let results = test_store.query(query).await?;
        assert_eq!(results.entities.len(), 1);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn store_mutate_return_entities() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let mutation = test_store
            .create_put_contact_mutation("entry1", "trt1", "Hello World")
            .return_entities();
        let result = test_store.mutate(mutation).await?;
        assert_eq!(result.entities.len(), 1);

        let entity = &result.entities[0];
        assert_eq!(entity.id, "entry1");

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn store_mutate_generate_ids() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let test_msg = TestMessage::default().pack_to_any().unwrap();

        {
            // generate entity and trait ids if none are given
            let mutation = test_store
                .create_put_contact_mutation("", "trt1", "Hello World")
                .return_entities();
            let result = test_store.mutate(mutation).await?;
            assert_eq!(result.entities.len(), 1);
            assert_ne!("", result.entities[0].id);
            assert_ne!("", result.entities[0].traits[0].id);
        }

        {
            // use same entity id for both mutations if requested to be common ids
            let mutation = MutationBuilder::new()
                .put_trait(
                    "".to_string(),
                    Trait {
                        id: "".to_string(),
                        message: Some(test_msg.clone()),
                        ..Default::default()
                    },
                )
                .put_trait(
                    "".to_string(),
                    Trait {
                        id: "".to_string(),
                        message: Some(test_msg),
                        ..Default::default()
                    },
                )
                .use_common_entity_id()
                .return_entities();
            let result = test_store.mutate(mutation).await?;

            // should have created 1 entity with same id, with 2 different traits
            assert_eq!(result.entities.len(), 1);
            assert_ne!("", result.entities[0].id);
            assert_eq!(result.entities[0].traits.len(), 2);
            assert_ne!("", result.entities[0].traits[0].id);
            assert_ne!("", result.entities[0].traits[1].id);
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn query_error_propagating() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let query = QueryBuilder::test(false).build();
        assert!(test_store.query(query).await.is_err());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mutation_error_propagating() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let mutation = MutationBuilder::new().fail_mutation("entity_id".to_string());
        assert!(test_store.mutate(mutation).await.is_err());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn watched_query() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let query = QueryBuilder::matches("hello").build();
        let mut stream = block_on_stream(test_store.store_handle.watched_query(query)?);

        let result = stream.next().unwrap();
        assert_eq!(result.unwrap().entities.len(), 0);

        let mutation = test_store.create_put_contact_mutation("entry1", "trt1", "Hello World");
        let response = test_store.mutate(mutation).await?;
        test_store
            .cluster
            .wait_operation_committed(0, response.operation_ids[0]);

        let result = stream.next().unwrap();
        assert_eq!(result.unwrap().entities.len(), 1);

        let mutation =
            test_store.create_put_contact_mutation("entry2", "contact2", "Something else");
        let response = test_store.mutate(mutation).await?;
        test_store
            .cluster
            .wait_operation_committed(0, response.operation_ids[0]);

        let mut stream = stream.into_inner();
        let delay = sleep(Duration::from_secs(1));

        futures::select! {
            _ = stream.next().fuse() => {
                panic!("Got result, should have timed out");
            },
            _ = delay.fuse() => {
                // alright
            }
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn watched_query_failure() -> anyhow::Result<()> {
        let mut test_store = TestStore::new().await?;
        test_store.start_store().await?;

        let query = QueryBuilder::test(false).build();
        let mut stream = block_on_stream(test_store.store_handle.watched_query(query)?);

        let result = stream.next().unwrap();
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn garbage_collection() -> anyhow::Result<()> {
        let store_config = StoreConfig {
            garbage_collect_interval: Duration::from_millis(50),
            ..Default::default()
        };
        let index_config = EntityIndexConfig {
            chain_index_min_depth: 0, // index in chain as soon as a block is committed
            chain_index_depth_leeway: 0, // for tests, we want to index as soon as possible
            chain_index_in_memory: false,
            garbage_collector: GarbageCollectorConfig {
                deleted_entity_collection: Duration::from_millis(100),
                ..Default::default()
            },
            ..TestStore::test_index_config()
        };

        let mut test_store = TestStore::new_with_config(store_config, index_config).await?;
        test_store.start_store().await?;

        {
            // create an entity and then delete it
            let mutation =
                test_store.create_put_contact_mutation("entity1", "trt1", "Hello entity1");
            test_store.mutate(mutation).await?;

            let delete = MutationBuilder::new()
                .delete_entity("entity1")
                .return_entities()
                .build();
            let resp = test_store.mutate(delete).await?;
            test_store
                .cluster
                .wait_operation_committed(0, resp.operation_ids[0]);
        }

        {
            // entity should be deleted, but can still be returned if we request deleted
            // ones
            let query = QueryBuilder::all().build();
            let res = test_store.query(query).await?;
            assert_eq!(res.entities.len(), 0);

            let query = QueryBuilder::all().include_deleted().build();
            let res = test_store.query(query).await?;
            assert_eq!(res.entities.len(), 1);
        }

        // entity should eventually be completely deleted
        let store_handle = test_store.store_handle.clone();
        async_expect_eventually(|| async {
            let query = QueryBuilder::all().include_deleted().build();
            let res = store_handle.query(query).await.unwrap();
            res.entities.is_empty()
        })
        .await;

        Ok(())
    }
}
