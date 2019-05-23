use crate::errors::Error;

pub struct Store<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    _data_handle: exocore_data::engine::EngineHandle<CP, PP>,
}

impl<CP, PP> Store<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    fn start(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
