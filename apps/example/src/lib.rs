#[macro_use]
extern crate log;

use std::time::Duration;

use exocore_apps_sdk::time::{now, sleep};
use exocore_apps_sdk::{exocore_app, spawn, App, AppError, Exocore};

#[exocore_app]
pub struct MyApp {
    // TODO:
}

impl MyApp {
    fn new() -> Self {
        MyApp {}
    }
}

impl App for MyApp {
    fn start(&self, exocore: &Exocore) -> Result<(), AppError> {
        // TODO: Check if default objects are created
        info!("initialized!");

        let store = exocore.store.clone();
        spawn(async move {
            info!("inside future!");

            loop {
                sleep(Duration::from_millis(500)).await;
                info!("tick {}", now());
            }

            // let q = QueryBuilder::with_id("test").build();
            // let _ = store.query(q).await;
        });

        Ok(())
    }
}
