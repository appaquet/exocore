#[macro_use]
extern crate log;

use std::sync::Arc;

use exocore_core::logging::setup;
use exocore_protos::apps::OutMessage;
use runtime::{AppRuntime, Environment};

mod runtime;

fn main() -> anyhow::Result<()> {
    setup(None);
    struct MyEnv;
    impl Environment for MyEnv {
        fn handle_message(&self, out: OutMessage) {
            info!("Got out message: {:?}", out);
        }

        fn handle_log(&self, msg: &str) {
            info!("Got log: {}", msg);
        }
    }

    let app_runtime = AppRuntime::from_file("fixtures/example.wasm", Arc::new(MyEnv))?;
    app_runtime.run()?;
    Ok(())
}
