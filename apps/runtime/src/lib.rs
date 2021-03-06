#[macro_use]
extern crate log;

use std::{path::PathBuf, sync::Mutex};

use exocore_core::cell::Cell;
use exocore_protos::apps;
use exocore_store::store::Store;
use futures::channel::mpsc;

mod runtime;

const OUT_MESSAGE_SIZE: usize = 5000;

pub struct Applications<S: Store> {
    cell: Cell,
    store: S,
    apps: Vec<Application>,
}


impl<S: Store> Applications<S> {
    pub async fn new(cell: Cell, store: S) -> Applications {
        let apps_directory = cell.apps_directory();

        let mut apps = Vec::new();
        for app in cell.applications().applications() {
            let app = app.application();
            let app_manifest = app.manifest();

            let module = if let Some(module) = &app_manifest.module {
                module
            } else {
                continue
            };

            // TODO: check if we have the module already downloaded
            // TODO: check if it has the right hash

            // TODO: apps.push()
        }

        Applications {
            cell,
            store,
            apps,
        }
    }

    async fn run(self) {

        // TODO: Get messages back from store.
        // TODO: Tick
        // TODO: Get out message, send to store
    }
}

struct Application {
    app: exocore_core::cell::Application,
    module_path: PathBuf,
}

struct WiredEnvironment {
    sender: Mutex<mpsc::Sender<exocore_protos::apps::OutMessage>>,
}

impl runtime::HostEnvironment for WiredEnvironment {
    fn handle_message(&self, msg: exocore_protos::apps::OutMessage) {
        let mut sender = self.sender.lock().unwrap();
        if let Err(err) = sender.try_send(msg) {
            error!("Couldn't send message via channel: {}", err)
        }
    }

    fn handle_log(&self, level: log::Level, msg: &str) {
        log!(level, "WASM APP: {}", msg);
    }
}
