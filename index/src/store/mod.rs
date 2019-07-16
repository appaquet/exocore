pub mod local;
pub mod remote;

use crate::error::Error;
use crate::mutation::Mutation;
use crate::query::Query;
use crate::results::EntitiesResults;
use exocore_data::operation::OperationId;
use futures::Future;

///
///
///
pub type AsyncResult<I> = Box<dyn Future<Item = I, Error = Error> + Send>;

pub trait AsyncStore {
    fn mutate(&self, mutation: Mutation) -> AsyncResult<OperationId>;
    fn query(&self, query: Query) -> AsyncResult<EntitiesResults>;
}
