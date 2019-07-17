use crate::mutation::Mutation;
use crate::query::Query;
use crate::results::EntitiesResults;
use crate::store::{AsyncResult, AsyncStore};
use exocore_common::cell::Cell;
use exocore_data::operation::OperationId;
use exocore_transport::TransportHandle;

pub struct RemoteStore<T: TransportHandle> {
    _transport: T,
}

impl<T: TransportHandle> RemoteStore<T> {
    pub fn new(_transport: T, _cell: Cell) -> RemoteStore<T> {
        unimplemented!()
    }
}

impl<T: TransportHandle> AsyncStore for RemoteStore<T> {
    fn mutate(&self, _mutation: Mutation) -> AsyncResult<OperationId> {
        unimplemented!()
    }

    fn query(&self, _query: Query) -> AsyncResult<EntitiesResults> {
        unimplemented!()
    }
}
