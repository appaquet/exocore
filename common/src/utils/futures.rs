use futures01::Future as Future01;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use futures::compat::Future01CompatExt;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use futures::TryFutureExt;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn spawn_future<F>(f: F) -> tokio::executor::Spawn
where
    F: Future01<Item = (), Error = ()> + 'static + Send,
{
    tokio::executor::spawn(f)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn spawn_future_non_send<F>(_f: F)
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    unimplemented!()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn spawn_future<F>(f: F)
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(f.compat().unwrap_or_else(|_| ()));
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub fn spawn_future_non_send<F>(f: F)
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(f.compat().unwrap_or_else(|_| ()));
}
