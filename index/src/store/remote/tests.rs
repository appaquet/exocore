use std::time::Duration;

use futures::prelude::*;

use exocore_common::node::LocalNode;
use exocore_common::tests_utils::expect_eventually;
use exocore_transport::mock::MockTransportHandle;
use exocore_transport::TransportLayer;

use crate::error::Error;
use crate::mutation::{Mutation, MutationResult, TestFailMutation};
use crate::query::{Query, QueryResult};
use crate::store::local::TestLocalStore;
use crate::store::{AsyncStore, ResultStream};

use super::*;
use crate::store::remote::server::StoreServer;

#[test]
fn mutation_and_query() -> Result<(), failure::Error> {
    let mut test_remote_store = TestRemoteStore::new()?;
    test_remote_store.start_server()?;
    test_remote_store.start_client()?;

    let mutation = test_remote_store
        .local_store
        .create_put_contact_mutation("entity1", "trait1", "hello");
    test_remote_store.send_and_await_mutation(mutation)?;

    expect_eventually(|| {
        let query = Query::match_text("hello");
        let results = test_remote_store.send_and_await_query(query).unwrap();
        results.results.len() == 1
    });

    Ok(())
}

#[test]
fn mutation_error_propagation() -> Result<(), failure::Error> {
    let mut test_remote_store = TestRemoteStore::new()?;
    test_remote_store.start_server()?;
    test_remote_store.start_client()?;

    let mutation = Mutation::TestFail(TestFailMutation {});
    let result = test_remote_store.send_and_await_mutation(mutation);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn query_error_propagation() -> Result<(), failure::Error> {
    let mut test_remote_store = TestRemoteStore::new()?;
    test_remote_store.start_server()?;
    test_remote_store.start_client()?;

    let mutation = test_remote_store
        .local_store
        .create_put_contact_mutation("entity1", "trait1", "hello");
    test_remote_store.send_and_await_mutation(mutation)?;

    let query = Query::test_fail();
    let result = test_remote_store.send_and_await_query(query);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn query_timeout() -> Result<(), failure::Error> {
    let config = ClientConfiguration {
        query_timeout: Duration::from_millis(500),
        ..ClientConfiguration::default()
    };

    let mut test_remote_store = TestRemoteStore::new_with_configuration(config)?;

    // only start remote, so local won't answer and it should timeout
    test_remote_store.start_client()?;

    let query = Query::match_text("hello");
    let result = test_remote_store.send_and_await_query(query);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn mutation_timeout() -> Result<(), failure::Error> {
    let config = ClientConfiguration {
        mutation_timeout: Duration::from_millis(500),
        ..ClientConfiguration::default()
    };

    let mut test_remote_store = TestRemoteStore::new_with_configuration(config)?;

    // only start remote, so local won't answer and it should timeout
    test_remote_store.start_client()?;

    let mutation = test_remote_store
        .local_store
        .create_put_contact_mutation("entity1", "trait1", "hello");
    let result = test_remote_store.send_and_await_mutation(mutation);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn watched_query() -> Result<(), failure::Error> {
    let mut test_remote_store = TestRemoteStore::new()?;
    test_remote_store.start_server()?;
    test_remote_store.start_client()?;

    let mutation = test_remote_store
        .local_store
        .create_put_contact_mutation("entity1", "trait1", "hello");
    test_remote_store.send_and_await_mutation(mutation)?;

    let query = Query::match_text("hello");
    let stream = test_remote_store.client_handle.watched_query(query);

    let (results, stream) = test_remote_store.get_stream_result(stream);
    let results = results.unwrap();
    assert_eq!(results.results.len(), 1);

    let mutation = test_remote_store
        .local_store
        .create_put_contact_mutation("entity2", "trait2", "hello");
    test_remote_store.send_and_await_mutation(mutation)?;

    let (results, _stream) = test_remote_store.get_stream_result(stream);
    assert_eq!(results.unwrap().results.len(), 2);

    Ok(())
}

#[test]
fn watched_query_timeout() -> Result<(), failure::Error> {
    Ok(())
}

#[test]
fn watched_query_error_propagation() -> Result<(), failure::Error> {
    Ok(())
}

struct TestRemoteStore {
    local_store: TestLocalStore,
    client: Option<StoreClient<MockTransportHandle>>,
    client_handle: ClientHandle,
}

impl TestRemoteStore {
    fn new() -> Result<TestRemoteStore, failure::Error> {
        let config = ClientConfiguration::default();
        Self::new_with_configuration(config)
    }

    fn new_with_configuration(
        config: ClientConfiguration,
    ) -> Result<TestRemoteStore, failure::Error> {
        let local_store = TestLocalStore::new()?;

        let local_node = LocalNode::generate();
        let store_client = StoreClient::new(
            config,
            local_store.cluster.cells[0].cell().clone(),
            local_store.cluster.clocks[0].clone(),
            local_store.schema.clone(),
            local_store
                .cluster
                .transport_hub
                .get_transport(local_node, TransportLayer::Index),
            local_store.cluster.nodes[0].node().clone(),
        )?;
        let client_handle = store_client.get_handle()?;

        Ok(TestRemoteStore {
            local_store,
            client: Some(store_client),
            client_handle,
        })
    }

    fn start_server(&mut self) -> Result<(), failure::Error> {
        let store_handle = self.local_store.store.as_ref().unwrap().get_handle();

        self.local_store.start_store()?;

        let cell = self.local_store.cluster.cells[0].cell().clone();
        let schema = self.local_store.schema.clone();
        let transport = self.local_store.cluster.transport_hub.get_transport(
            self.local_store.cluster.nodes[0].clone(),
            TransportLayer::Index,
        );

        let server = StoreServer::new(cell, schema, store_handle, transport)?;

        self.local_store
            .cluster
            .runtime
            .spawn(server.map_err(|err| {
                error!("Error spawning remote store server: {}", err);
            }));

        // TODO: Wait on start

        Ok(())
    }

    fn start_client(&mut self) -> Result<(), failure::Error> {
        let client = self.client.take().unwrap();
        self.local_store
            .cluster
            .runtime
            .spawn(client.map_err(|err| {
                error!("Error spawning remote store: {}", err);
            }));

        self.local_store
            .cluster
            .runtime
            .block_on(self.client_handle.on_start()?)?;

        Ok(())
    }

    fn send_and_await_mutation(&mut self, mutation: Mutation) -> Result<MutationResult, Error> {
        self.local_store
            .cluster
            .runtime
            .block_on(self.client_handle.mutate(mutation))
    }

    fn send_and_await_query(&mut self, query: Query) -> Result<QueryResult, Error> {
        self.local_store
            .cluster
            .runtime
            .block_on(self.client_handle.query(query))
    }

    fn get_stream_result(
        &mut self,
        stream: ResultStream<QueryResult>,
    ) -> (Option<QueryResult>, ResultStream<QueryResult>) {
        self.local_store
            .cluster
            .runtime
            .block_on(stream.into_future())
            .map_err(|_| ())
            .unwrap()
    }
}
