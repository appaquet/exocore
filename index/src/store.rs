use crate::error::Error;
use crate::mutation::Mutation;
use crate::query::Query;
use crate::results::EntitiesResults;
use exocore_data::operation::OperationId;
use futures::sync::oneshot;
use futures::{Async, Future};

///
///
///
pub type AsyncResult<I> = Box<dyn Future<Item = I, Error = Error> + Send>;

pub trait AsyncStore {
    fn mutate(&self, mutation: Mutation) -> AsyncResult<OperationId>;
    fn query(&self, query: Query) -> AsyncResult<EntitiesResults>;
}

///
///
///
pub struct QueryResolver {
    receiver: oneshot::Receiver<Result<EntitiesResults, Error>>,
}

impl QueryResolver {
    pub fn new(receiver: oneshot::Receiver<Result<EntitiesResults, Error>>) -> QueryResolver {
        QueryResolver { receiver }
    }
}

impl Future for QueryResolver {
    type Item = EntitiesResults;
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        let result = self
            .receiver
            .poll()
            .map_err(|_err| Error::Other("Query got cancelled".to_string()))?;

        match result {
            Async::Ready(Ok(results)) => Ok(Async::Ready(results)),
            Async::Ready(Err(err)) => Err(err),
            Async::NotReady => Ok(Async::NotReady),
        }
    }
}
