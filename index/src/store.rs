use exocore_data;

// TODO: Needs to decrypt
/* TODO: Use thread local for decryption + decompression.
        Impose a limit on decrypt and decompress block size...
        https://doc.rust-lang.org/std/macro.thread_local.html
*/

pub struct Store<T, CP, PP>
where
    T: exocore_transport::TransportHandle,
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    _data_engine: exocore_data::Engine<T, CP, PP>,
}

impl<T, CP, PP> Store<T, CP, PP>
where
    T: exocore_transport::TransportHandle,
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{

    // TODO : Instanciate Tantivy

    pub fn handle_stream() -> () {

    }

    fn index

}
