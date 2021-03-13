#[macro_use]
extern crate log;

use std::{io::Write, path::PathBuf, sync::Arc, time::Duration};

use anyhow::anyhow;
use error::Error;
use exocore_core::{
    cell::Cell,
    futures::{block_on, owned_spawn, sleep, spawn_blocking, spawn_future, BatchingStream},
};
use exocore_protos::{
    apps::{self, in_message::InMessageType, out_message::OutMessageType, InMessage, OutMessage},
    prost::{Message, ProstMessageExt},
    store::{EntityMutation, EntityQuery},
};
use exocore_store::store::Store;
use futures::{
    channel::mpsc,
    future::{select_all, FutureExt},
    lock::Mutex,
    Future, SinkExt, StreamExt,
};
use runtime::AppRuntime;

mod error;
mod runtime;

const MSG_BUFFER_SIZE: usize = 5000;
const RUNTIME_MSG_BATCH_SIZE: usize = 1000;

pub struct Applications<S: Store> {
    store: S,
    apps: Vec<Application>,
}

impl<S: Store + Clone + Send + 'static> Applications<S> {
    pub async fn new(cell: Cell, store: S) -> Result<Applications<S>, Error> {
        let apps_directory = cell
            .apps_directory()
            .ok_or_else(|| anyhow!("Cell doesn't have an apps directory"))?;

        let mut apps = Vec::new();
        for app in cell.applications().applications() {
            let app = app.application();
            let app_manifest = app.manifest();

            let module_manifest = if let Some(module) = &app_manifest.module {
                module
            } else {
                continue;
            };

            let module_path = apps_directory.join(format!(
                "{}_{}/module.wasm",
                app_manifest.name, app_manifest.version
            ));
            maybe_download_module(&module_path, app_manifest, module_manifest).await?;

            apps.push(Application {
                _app: app.clone(),
                module_path,
            });
        }

        Ok(Applications { store, apps })
    }

    pub async fn run(self) -> Result<(), Error> {
        let mut spawn_set = Vec::new();

        for app in self.apps {
            spawn_set.push(owned_spawn(Self::start_app_loop(app, self.store.clone())));
        }

        // TODO: Get messages back from store.
        // TODO: Tick
        // TODO: Get out message, send to store

        let _ = select_all(spawn_set).await;

        Ok(())
    }

    async fn start_app_loop(app: Application, store: S) {
        loop {
            let (in_sender, in_receiver) = mpsc::channel(MSG_BUFFER_SIZE);
            let (out_sender, mut out_receiver) = mpsc::channel(MSG_BUFFER_SIZE);

            let env = Arc::new(WiredEnvironment {
                sender: std::sync::Mutex::new(out_sender),
            });

            let app_module_path = app.module_path.clone();
            let runtime_spawn = spawn_blocking(|| -> Result<(), Error> {
                let app_runtime = AppRuntime::from_file(app_module_path, env)?;
                let mut batch_receiver = BatchingStream::new(in_receiver, RUNTIME_MSG_BATCH_SIZE);

                const MIN_TICK_TIME: Duration = Duration::from_millis(100);
                let mut next_tick = sleep(MIN_TICK_TIME);
                loop {
                    let in_messages: Option<Vec<InMessage>> = block_on(async {
                        futures::select! {
                            _ = next_tick.fuse() => Some(vec![]),
                            msgs = batch_receiver.next().fuse() => msgs,
                        }
                    });

                    let in_messages = if let Some(in_messages) = in_messages {
                        in_messages
                    } else {
                        info!("In message receiver returned none. Stopping app runtime");
                        return Ok(());
                    };

                    for in_message in in_messages {
                        app_runtime.send_message(in_message)?;
                    }

                    let next_tick_duration = app_runtime.tick()?.unwrap_or(MIN_TICK_TIME);
                    next_tick = sleep(next_tick_duration);
                }
            });

            let store = store.clone();
            let store_worker = async move {
                let in_sender = Arc::new(Mutex::new(in_sender));
                while let Some(message) = out_receiver.next().await {
                    match OutMessageType::from_i32(message.r#type) {
                        Some(OutMessageType::StoreEntityQuery) => {
                            let store = store.clone();
                            handle_store_message(
                                message.rendez_vous_id,
                                InMessageType::StoreEntityResults,
                                in_sender.clone(),
                                move || handle_entity_query(message, store),
                            )
                        }
                        Some(OutMessageType::StoreMutationRequest) => {
                            let store = store.clone();
                            handle_store_message(
                                message.rendez_vous_id,
                                InMessageType::StoreMutationResult,
                                in_sender.clone(),
                                move || handle_entity_mutation(message, store),
                            )
                        }
                        other => {
                            error!(
                                "Got an unknown message type {:?} with id {}",
                                other, message.r#type
                            );
                        }
                    }
                }

                Ok::<(), Error>(())
            };

            futures::select! {
                res = runtime_spawn.fuse() => {
                    info!("App runtime spawn has stopped: {:?}", res);
                }
                _ = store_worker.fuse() => {
                    info!("Store worker task has stopped");
                }
            };
        }
    }
}

fn handle_store_message<F, O>(
    rendez_vous_id: u32,
    reply_type: InMessageType,
    in_sender: Arc<Mutex<mpsc::Sender<InMessage>>>,
    func: F,
) where
    F: (FnOnce() -> O) + Send + 'static,
    O: Future<Output = Result<Vec<u8>, Error>> + Send + 'static,
{
    spawn_future(async move {
        let mut msg = InMessage {
            r#type: reply_type.into(),
            rendez_vous_id,
            ..Default::default()
        };

        let res = func();
        match res.await {
            Ok(res) => msg.data = res,
            Err(err) => msg.error = err.to_string(),
        }

        let mut in_sender = in_sender.lock().await;
        let _ = in_sender.send(msg).await;
    });
}

async fn handle_entity_query<S: Store + Send + 'static>(
    out_message: OutMessage,
    store: S,
) -> Result<Vec<u8>, Error> {
    let query = EntityQuery::decode(out_message.data.as_ref())?;
    let res = store.query(query);
    let res = res.await?;

    Ok(res.encode_to_vec())
}

async fn handle_entity_mutation<S: Store + Send + 'static>(
    out_message: OutMessage,
    store: S,
) -> Result<Vec<u8>, Error> {
    let mutation = EntityMutation::decode(out_message.data.as_ref())?;
    let res = store.mutate(mutation);
    let res = res.await?;

    Ok(res.encode_to_vec())
}

struct Application {
    _app: exocore_core::cell::Application,
    module_path: PathBuf,
}

struct WiredEnvironment {
    sender: std::sync::Mutex<mpsc::Sender<exocore_protos::apps::OutMessage>>,
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

async fn maybe_download_module(
    module_path: &PathBuf,
    app_manifest: &apps::Manifest,
    module: &apps::ManifestModule,
) -> Result<(), Error> {
    // TODO: Check checksum
    let module_exists = std::fs::metadata(&module_path).is_ok();
    if module_exists {
        return Ok(());
    }

    if let Some(parent) = module_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| anyhow!("Couldn't create module directory '{:?}': {}", parent, err))?;
    }

    if module.url.starts_with("file://") {
        let source_path = module.url.strip_prefix("file://").unwrap();
        std::fs::copy(source_path, module_path).map_err(|err| {
            anyhow!(
                "Couldn't copy app module from '{:?}' to path '{:?}': {}",
                source_path,
                module_path,
                err
            )
        })?;
        return Ok(());
    }

    let body = reqwest::get(&module.url)
        .await
        .map_err(|err| {
            anyhow!(
                "Couldn't download app {} module from {}: {}",
                app_manifest.name,
                module.url,
                err
            )
        })?
        .bytes()
        .await
        .map_err(|err| {
            anyhow!(
                "Couldn't download app {} module from {}: {}",
                app_manifest.name,
                module.url,
                err
            )
        })?;

    let mut file = std::fs::File::create(&module_path)
        .map_err(|err| anyhow!("Couldn't create module app file: {}", err))?;
    file.write_all(body.as_ref())
        .map_err(|err| anyhow!("Couldn't write app file: {}", err))?;

    Ok(())
}
