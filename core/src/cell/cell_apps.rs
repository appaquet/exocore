use super::{Application, ApplicationId, Error};
use crate::protos::generated::exocore_core::{cell_application_config, CellApplicationConfig};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct CellApplications {
    applications: Arc<RwLock<HashMap<ApplicationId, CellApplication>>>,
}

impl CellApplications {
    pub(crate) fn new() -> CellApplications {
        CellApplications {
            applications: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) fn load_from_cell_applications_config<'c, I>(&self, iter: I) -> Result<(), Error>
    where
        I: Iterator<Item = &'c CellApplicationConfig> + 'c,
    {
        let mut apps = self.applications.write().unwrap();

        for cell_app in iter {
            let any_manifest = cell_app.manifest.as_ref().ok_or_else(|| {
                Error::Config("CellApplication needs a manifest to be definied".to_string())
            })?;

            match any_manifest {
                cell_application_config::Manifest::Instance(manifest) => {
                    let application = Application::new_from_manifest(manifest)?;
                    apps.insert(application.id().clone(), CellApplication { application });
                }
                cell_application_config::Manifest::YamlFile(_) => {
                    return Err(Error::Config(
                        "Manifest from yaml file not supported".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

pub struct CellApplication {
    application: Application,
}

impl CellApplication {
    pub fn application(&self) -> &Application {
        &self.application
    }
}
