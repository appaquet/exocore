use async_trait::async_trait;
use exocore_protos::store::{EntityQuery, EntityResults, MutationResult};
use futures::Stream;

use crate::{error::Error, mutation::MutationRequestLike};

#[async_trait]
pub trait Store {
    type WatchedQueryStreamT: Stream<Item = Result<EntityResults, Error>>;

    async fn mutate<M: Into<MutationRequestLike> + Send + Sync>(
        &self,
        request: M,
    ) -> Result<MutationResult, Error>;

    async fn query(&self, query: EntityQuery) -> Result<EntityResults, Error>;

    fn watched_query(&self, query: EntityQuery) -> Result<Self::WatchedQueryStreamT, Error>;
}
