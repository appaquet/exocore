use exocore_apps_sdk::{App, AppError, Exocore, exocore_app, spawn};
use exocore_store::query::QueryBuilder;

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

        let store = exocore.store.clone();
        spawn(async move {
            let q = QueryBuilder::with_id("test").build();
            let _ = store.query(q).await;
        });
        
        Ok(())
    }
}


