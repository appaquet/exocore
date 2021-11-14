use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use exocore_protos::{
    generated::exocore_core::{cell_application_config, CellApplicationConfig},
    registry::Registry,
};

use crate::dir::DynDirectory;

use super::{Application, ApplicationId, Error};

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

    // TODO: Should be DynDirectory direct, not option
    pub(crate) fn load_from_cell_apps_conf<'c, I>(
        &self,
        apps_directory: Option<DynDirectory>,
        iter: I,
    ) -> Result<(), Error>
    where
        I: Iterator<Item = &'c CellApplicationConfig> + 'c,
    {
        for cell_app in iter {
            if let Some(apps_dir) = &apps_directory {
                let app_dir =
                    cell_app_directory(apps_dir, &cell_app.public_key, &cell_app.version)?;
                let app = Application::from_directory(app_dir)?;
                self.add_application(app)?;
                continue;
            }

            // TODO: Remove the code bellow when everything is migrated
            let app_location = if let Some(loc) = &cell_app.location {
                loc
            } else {
                warn!(
                    "Cannot load application {} (version {}). No location configured.",
                    cell_app.name, cell_app.version
                );
                continue;
            };

            match app_location {
                cell_application_config::Location::Inline(manifest) => {
                    let application = Application::from_manifest(manifest.clone())?;
                    self.add_application(application)?;
                }
                cell_application_config::Location::Path(dir) => {
                    let application = Application::from_directory_old(&dir)?;
                    self.add_application(application)?;
                }
            }
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
    app_public_key: &str,
    app_version: &str,
) -> Result<DynDirectory, Error> {
    let dir = apps_dir.scope(PathBuf::from(format!("{}_{}", app_public_key, app_version)))?;
    Ok(dir)
}
