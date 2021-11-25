use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use exocore_protos::{generated::exocore_core::CellApplicationConfig, registry::Registry};

use super::{Application, ApplicationId, Error};
use crate::dir::DynDirectory;

/// Applications installed in a cell.
#[derive(Clone)]
pub struct CellApplications {
    applications: Arc<RwLock<HashMap<ApplicationId, CellApplication>>>,
    schemas: Arc<Registry>,
}

impl CellApplications {
    pub(crate) fn new(schemas: Arc<Registry>) -> CellApplications {
        CellApplications {
            applications: Arc::new(RwLock::new(HashMap::new())),
            schemas,
        }
    }

    pub(crate) fn load_from_configurations<'c, I>(
        &self,
        apps_dir: &DynDirectory,
        iter: I,
    ) -> Result<(), Error>
    where
        I: Iterator<Item = &'c CellApplicationConfig> + 'c,
    {
        for cell_app in iter {
            let app_id = ApplicationId::from_base58_public_key(&cell_app.public_key)?;
            let app_dir = cell_app_directory(apps_dir, &app_id, &cell_app.version);

            // TODO: Should skip if not on disk
            
            let app = Application::from_directory(app_dir).map_err(|err| {
                Error::Application(
                    cell_app.name.clone(),
                    anyhow!("failed to load from directory: {}", err),
                )
            })?;
            self.add_application(app)?;
        }

        Ok(())
    }

    pub fn add_application(&self, application: Application) -> Result<(), Error> {
        let mut apps = self.applications.write().unwrap();

        for fd_set in application.schemas() {
            self.schemas.register_file_descriptor_set(fd_set);
        }

        apps.insert(application.id().clone(), CellApplication { application });
        Ok(())
    }

    pub fn applications(&self) -> Vec<CellApplication> {
        let apps = self.applications.read().expect("couldn't lock inner");
        apps.values().cloned().collect()
    }
}

#[derive(Clone)]
pub struct CellApplication {
    application: Application,
}

impl CellApplication {
    pub fn application(&self) -> &Application {
        &self.application
    }
}

impl Deref for CellApplication {
    type Target = Application;

    fn deref(&self) -> &Self::Target {
        &self.application
    }
}

pub fn cell_app_directory(
    apps_dir: &DynDirectory,
    app_id: &ApplicationId,
    app_version: &str,
) -> DynDirectory {
    apps_dir.scope(PathBuf::from(format!("{}_{}", app_id, app_version)))
}
