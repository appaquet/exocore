use std::{fs::File, io::prelude::*, path::Path};

use exocore_protos::{
    core::{cell_application_config, cell_node_config, CellApplicationConfig, NodeConfig},
    generated::{
        exocore_apps::Manifest,
        exocore_core::{
            node_cell_config, CellConfig, CellNodeConfig, LocalNodeConfig, NodeCellConfig,
        },
    },
};

use super::Error;

/// Extension for `LocalNodeConfig` proto.
pub trait LocalNodeConfigExt {
    fn config(&self) -> &LocalNodeConfig;

    fn from_yaml_reader<R: Read>(reader: R) -> Result<LocalNodeConfig, Error>;

    // TODO: Replace with file system
    fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<LocalNodeConfig, Error>;

    fn from_json_reader<R: Read>(bytes: R) -> Result<LocalNodeConfig, Error>;

    fn to_yaml(&self) -> Result<String, Error>;

    fn to_yaml_writer<W: Write>(&self, write: W) -> Result<(), Error>;

    // TODO: Remove me
    fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error>;

    fn to_json(&self) -> Result<String, Error>;

    fn inlined(&self) -> Result<LocalNodeConfig, Error>;

    fn create_cell_node_config(&self, roles: Vec<cell_node_config::Role>) -> CellNodeConfig;

    fn add_cell(&mut self, cell: NodeCellConfig);
}

impl LocalNodeConfigExt for LocalNodeConfig {
    fn config(&self) -> &LocalNodeConfig {
        self
    }

    fn from_yaml_reader<R: Read>(reader: R) -> Result<LocalNodeConfig, Error> {
        let config = serde_yaml::from_reader(reader)
            .map_err(|err| Error::Config(anyhow!("Couldn't decode YAML node config: {}", err)))?;

        Ok(config)
    }

    fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<LocalNodeConfig, Error> {
        let file = File::open(path.as_ref()).map_err(|err| {
            Error::Config(anyhow!(
                "Couldn't open YAML node file at path '{:?}': {}",
                path.as_ref(),
                err
            ))
        })?;

        Self::from_yaml_reader(file)
    }

    fn from_json_reader<R: Read>(bytes: R) -> Result<LocalNodeConfig, Error> {
        let config = serde_json::from_reader(bytes)
            .map_err(|err| Error::Config(anyhow!("Couldn't decode JSON node config: {}", err)))?;

        Ok(config)
    }

    fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self.config())
            .map_err(|err| Error::Config(anyhow!("Couldn't encode node config to YAML: {}", err)))
    }

    fn to_yaml_writer<W: Write>(&self, write: W) -> Result<(), Error> {
        serde_yaml::to_writer(write, self.config())
            .map_err(|err| Error::Config(anyhow!("Couldn't encode node config to YAML: {}", err)))
    }

    fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let file = File::create(path.as_ref()).map_err(|err| {
            Error::Config(anyhow!(
                "Couldn't create YAML node file at path '{:?}': {}",
                path.as_ref(),
                err
            ))
        })?;

        let config = self.clone();
        config.to_yaml_writer(file)?;

        Ok(())
    }

    fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self.config())
            .map_err(|err| Error::Config(anyhow!("Couldn't encode node config to JSON: {}", err)))
    }

    fn inlined(&self) -> Result<LocalNodeConfig, Error> {
        let mut config = self.config().clone();

        let mut cells = Vec::new();
        for node_cell_config in &config.cells {
            let cell_config = CellConfig::from_node_cell(node_cell_config)?;

            let mut node_cell_config = node_cell_config.clone();
            node_cell_config.location = Some(node_cell_config::Location::Inline(cell_config));

            cells.push(node_cell_config);
        }

        config.cells = cells;

        Ok(config)
    }

    fn create_cell_node_config(&self, roles: Vec<cell_node_config::Role>) -> CellNodeConfig {
        let node_config = self.config();
        CellNodeConfig {
            node: Some(NodeConfig {
                public_key: node_config.public_key.clone(),
                id: node_config.id.clone(),
                name: node_config.name.clone(),
                addresses: node_config.addresses.clone(),
            }),
            roles: roles.into_iter().map(|r| r.into()).collect(),
        }
    }

    fn add_cell(&mut self, cell: NodeCellConfig) {
        self.cells.retain(|other| other.id != cell.id);
        self.cells.push(cell);
    }
}

/// Extension for `CellNodeConfig` proto.
pub trait CellNodeConfigExt {
    fn config(&self) -> &CellNodeConfig;

    fn to_yaml(&self) -> Result<String, Error>;

    fn from_yaml<R: Read>(bytes: R) -> Result<CellNodeConfig, Error>;
}

impl CellNodeConfigExt for CellNodeConfig {
    fn config(&self) -> &CellNodeConfig {
        self
    }

    fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self.config()).map_err(|err| {
            Error::Config(anyhow!("Couldn't encode cell node config to YAML: {}", err))
        })
    }

    fn from_yaml<R: Read>(bytes: R) -> Result<CellNodeConfig, Error> {
        let config = serde_yaml::from_reader(bytes).map_err(|err| {
            Error::Config(anyhow!("Couldn't decode YAML cell node config: {}", err))
        })?;

        Ok(config)
    }
}

pub trait CellConfigExt {
    fn config(&self) -> &CellConfig;

    fn from_yaml<R: Read>(bytes: R) -> Result<CellConfig, Error>;

    fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<CellConfig, Error>;

    fn to_yaml(&self) -> Result<String, Error>;

    fn to_yaml_writer<W: Write>(&self, write: W) -> Result<(), Error>;

    fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error>;

    fn from_node_cell(config: &NodeCellConfig) -> Result<CellConfig, Error>;

    fn find_node(&mut self, node_pk: &str) -> Option<&mut CellNodeConfig>;

    fn add_node(&mut self, node: CellNodeConfig);

    fn add_application(&mut self, cell_app: CellApplicationConfig);
}

impl CellConfigExt for CellConfig {
    fn config(&self) -> &CellConfig {
        self
    }

    fn from_yaml<R: Read>(bytes: R) -> Result<CellConfig, Error> {
        let config: CellConfig = serde_yaml::from_reader(bytes)
            .map_err(|err| Error::Config(anyhow!("Couldn't decode YAML cell config: {}", err)))?;

        Ok(config)
    }

    fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<CellConfig, Error> {
        let file = File::open(path.as_ref()).map_err(|err| {
            Error::Config(anyhow!(
                "Couldn't open YAML cell config at path '{:?}': {}",
                path.as_ref(),
                err
            ))
        })?;

        Self::from_yaml(file)
    }

    fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self.config())
            .map_err(|err| Error::Config(anyhow!("Couldn't encode cell config to YAML: {}", err)))
    }

    fn to_yaml_writer<W: Write>(&self, write: W) -> Result<(), Error> {
        serde_yaml::to_writer(write, self.config())
            .map_err(|err| Error::Config(anyhow!("Couldn't encode cell config to YAML: {}", err)))
    }

    fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let file = File::create(path.as_ref()).map_err(|err| {
            Error::Config(anyhow!(
                "Couldn't open YAML node file at path '{:?}': {}",
                path.as_ref(),
                err
            ))
        })?;

        let config = self.clone();
        config.to_yaml_writer(file)?;

        Ok(())
    }

    fn from_node_cell(config: &NodeCellConfig) -> Result<CellConfig, Error> {
        match &config.location {
            Some(node_cell_config::Location::Inline(cell_config)) => Ok(cell_config.clone()),
            other => Err(Error::Config(anyhow!(
                "Invalid cell instance config: {:?}",
                other
            ))),
        }
    }

    fn find_node(&mut self, node_pk: &str) -> Option<&mut CellNodeConfig> {
        for cell_node in &mut self.nodes {
            if cell_node
                .node
                .as_ref()
                .map_or(false, |n| n.public_key == node_pk)
            {
                return Some(cell_node);
            }
        }

        None
    }

    fn add_node(&mut self, node: CellNodeConfig) {
        let node_pk = if let Some(node_pk) = node.node.as_ref().map(|n| n.public_key.as_str()) {
            node_pk
        } else {
            return;
        };

        // check if node exists first
        if let Some(cell_node) = self.find_node(node_pk) {
            *cell_node = node;
            return;
        }

        // otherwise it doesn't exist, we just add it
        self.nodes.push(node);
    }

    /// Adds application to the cell. Only support deduping on inline apps.
    fn add_application(&mut self, cell_app: CellApplicationConfig) {
        // check if app exists, and replace with newer is so
        for existing_cell_app in &mut self.apps {
            if cell_app.public_key == existing_cell_app.public_key {
                *existing_cell_app = cell_app;
                return;
            }
        }

        self.apps.push(cell_app);
    }
}

/// Extension for `NodeConfig` proto.
pub trait NodeConfigExt {
    fn from_yaml<R: Read>(bytes: R) -> Result<NodeConfig, Error>;

    fn to_yaml(&self) -> Result<String, Error>;
}

impl NodeConfigExt for NodeConfig {
    fn from_yaml<R: Read>(bytes: R) -> Result<NodeConfig, Error> {
        let config: NodeConfig = serde_yaml::from_reader(bytes)
            .map_err(|err| Error::Config(anyhow!("Couldn't decode YAML node config: {}", err)))?;

        Ok(config)
    }

    fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self).map_err(|err| {
            Error::Config(anyhow!("Couldn't encode cell node config to YAML: {}", err))
        })
    }
}

/// Extension for `CellApplicationConfig` proto.
pub trait CellApplicationConfigExt {
    fn from_manifest(manifest: Manifest) -> CellApplicationConfig;
}

impl CellApplicationConfigExt for CellApplicationConfig {
    fn from_manifest(manifest: Manifest) -> CellApplicationConfig {
        CellApplicationConfig {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            public_key: manifest.public_key.clone(),
            package_url: String::new(),
            location: Some(cell_application_config::Location::Inline(manifest)),
        }
    }
}

/// Extension for `Manifest` proto.
pub trait ManifestExt {
    fn manifest(&self) -> &Manifest;

    fn inlined(&self) -> Result<Manifest, Error>;

    fn from_yaml<R: Read>(bytes: R) -> Result<Manifest, Error>;

    fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Manifest, Error>;

    fn to_yaml_writer<W: Write>(&self, write: W) -> Result<(), Error>;
}

impl ManifestExt for Manifest {
    fn manifest(&self) -> &Manifest {
        self
    }

    fn inlined(&self) -> Result<Manifest, Error> {
        let mut app_manifest = self.manifest().clone();

        let app_name = app_manifest.name.clone();
        for schema in app_manifest.schemas.iter_mut() {
            let final_source = match schema.source.take() {
                Some(exocore_protos::apps::manifest_schema::Source::File(schema_path)) => {
                    let mut file = File::open(&schema_path).map_err(|err| {
                        Error::Application(
                            app_name.clone(),
                            anyhow!(
                                "Couldn't open application schema at path '{:?}': {}",
                                schema_path,
                                err
                            ),
                        )
                    })?;

                    let mut content = vec![];
                    file.read_to_end(&mut content).map_err(|err| {
                        Error::Application(
                            app_name.clone(),
                            anyhow!(
                                "Couldn't read application schema at path '{:?}': {}",
                                schema_path,
                                err
                            ),
                        )
                    })?;
                    exocore_protos::apps::manifest_schema::Source::Bytes(content)
                }
                Some(src @ exocore_protos::apps::manifest_schema::Source::Bytes(_)) => src,
                other => {
                    return Err(Error::Application(
                        app_name,
                        anyhow!("Unsupported application schema type: {:?}", other),
                    ));
                }
            };

            schema.source = Some(final_source);
        }

        Ok(app_manifest)
    }

    fn from_yaml<R: Read>(bytes: R) -> Result<Manifest, Error> {
        let config: Manifest = serde_yaml::from_reader(bytes)
            .map_err(|err| Error::Config(anyhow!("Couldn't decode YAML manifest: {}", err)))?;

        Ok(config)
    }

    fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Manifest, Error> {
        let path = path.as_ref();

        let file = File::open(path).map_err(|err| {
            Error::Application(
                "unnamed".to_string(),
                anyhow!(
                    "Couldn't open application manifest at path '{:?}': {}",
                    path,
                    err
                ),
            )
        })?;

        Self::from_yaml(file).map_err(|err| {
            Error::Application(
                "unnamed".to_string(),
                anyhow!(
                    "Couldn't decode YAML manifest at path '{:?}': {}",
                    path,
                    err
                ),
            )
        })
    }

    fn to_yaml_writer<W: Write>(&self, write: W) -> Result<(), Error> {
        serde_yaml::to_writer(write, self.manifest()).map_err(|err| {
            Error::Config(anyhow!(
                "Couldn't encode application manifest to YAML: {}",
                err
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use exocore_protos::{
        core::{
            cell_application_config, CellApplicationConfig, ChainConfig, EntityIndexConfig,
            MutationIndexConfig, NodeAddresses,
        },
        generated::exocore_core::{
            cell_node_config, node_cell_config, CellConfig, CellNodeConfig, LocalNodeConfig,
            NodeCellConfig, NodeConfig,
        },
    };

    use super::{
        super::{Cell, CellNodes},
        *,
    };
    use crate::{dir::os::OsDirectory, tests_utils::find_test_fixture};

    #[test]
    fn parse_node_config_yaml_ser_deser() -> anyhow::Result<()> {
        use exocore_protos::generated::exocore_core::NodeStoreConfig;

        let conf_ser = LocalNodeConfig {
            keypair: "keypair".to_string(),
            public_key: "pk".to_string(),
            name: "node_name".to_string(),
            id: String::new(),
            cells: vec![
                NodeCellConfig {
                    id: "cell1".into(),
                    location: Some(node_cell_config::Location::Inline(CellConfig {
                        public_key: "pk".to_string(),
                        keypair: "kp".to_string(),
                        name: "cell_name".to_string(),
                        id: String::new(),
                        nodes: vec![CellNodeConfig {
                            node: Some(NodeConfig {
                                public_key: "pk".to_string(),
                                name: "node_name".to_string(),
                                id: String::new(),
                                addresses: Some(NodeAddresses {
                                    p2p: vec!["maddr".to_string()],
                                    http: vec!["http_addr".to_string()],
                                }),
                            }),
                            roles: vec![cell_node_config::Role::ChainRole.into()],
                        }],
                        apps: vec![
                            CellApplicationConfig {
                                name: "app1".to_string(),
                                version: "0.0.1".to_string(),
                                public_key: "pk1".to_string(),
                                package_url: "https://somewhere/package.zip".to_string(),
                                location: Some(cell_application_config::Location::Inline(
                                    Manifest {
                                        name: "app1".to_string(),
                                        ..Default::default()
                                    },
                                )),
                            },
                            CellApplicationConfig {
                                name: "app2".to_string(),
                                version: "0.0.1".to_string(),
                                public_key: "pk2".to_string(),
                                package_url: "https://somewhere/package.zip".to_string(),
                                location: None,
                            },
                        ],
                    })),
                },
                NodeCellConfig {
                    id: "cell2".into(),
                    location: None,
                },
            ],
            addresses: Some(NodeAddresses {
                p2p: vec!["maddr".to_string()],
                http: vec!["http_addr".to_string()],
            }),
            listen_addresses: Some(NodeAddresses {
                p2p: vec!["listen_maddr".to_string()],
                http: vec!["listen_http_addr".to_string()],
            }),
            store: Some(NodeStoreConfig {
                index: Some(EntityIndexConfig {
                    chain_index_min_depth: Some(3),
                    chain_index_depth_leeway: Some(10),
                    pending_index: Some(MutationIndexConfig {
                        indexer_num_threads: Some(2),
                        indexer_heap_size_bytes: Some(30_000_000),
                        entity_mutations_cache_size: Some(2000),
                    }),
                    chain_index: Some(MutationIndexConfig {
                        indexer_num_threads: Some(2),
                        indexer_heap_size_bytes: Some(30_000_000),
                        entity_mutations_cache_size: Some(2000),
                    }),
                    ..Default::default()
                }),
                query_parallelism: Some(5),
            }),
            chain: Some(ChainConfig {
                segment_max_size: Some(1_000),
                segment_max_open_mmap: Some(2),
            }),
        };

        let conf_yaml = conf_ser.to_yaml()?;
        let conf_deser = LocalNodeConfig::from_yaml_reader(conf_yaml.as_bytes())?;
        assert_eq!(conf_ser, conf_deser);

        Ok(())
    }

    #[test]
    fn parse_node_config_example_yaml_file() -> anyhow::Result<()> {
        let node_path = find_test_fixture("examples/node");
        // let config = LocalNodeConfig::from_yaml_file(config_path)?;

        let dir = OsDirectory::new(node_path);
        let (cells, node) = Cell::from_local_node_directory(dir)?;
        assert_eq!(1, cells.len());
        assert_eq!(2, node.p2p_addresses().len());

        {
            // cell from directory
            let cell = cells[0].clone().unwrap_full();

            {
                let nodes = cell.cell().nodes();
                assert_eq!(2, nodes.count());
            }

            {
                let schemas = cell
                    .cell()
                    .schemas()
                    .get_message_descriptor("exocore.example_app.Task");
                assert!(schemas.is_ok());
            }
        }

        Ok(())
    }

    #[test]
    fn write_node_config_yaml_file() -> anyhow::Result<()> {
        let config_init = LocalNodeConfig {
            name: "node_name".to_string(),
            ..Default::default()
        };

        let dir = tempfile::tempdir()?;
        let file = dir.path().join("file");

        config_init.to_yaml_file(&file)?;

        let config_read = LocalNodeConfig::from_yaml_file(&file)?;

        assert_eq!(config_init, config_read);

        Ok(())
    }

    #[test]
    fn node_config_add_cell() {
        let mut config = LocalNodeConfig::default();

        config.add_cell(NodeCellConfig {
            id: "id1".into(),
            ..Default::default()
        });
        assert_eq!(1, config.cells.len());

        config.add_cell(NodeCellConfig {
            id: "id1".into(),
            ..Default::default()
        });
        assert_eq!(1, config.cells.len());

        config.add_cell(NodeCellConfig {
            id: "id2".into(),
            ..Default::default()
        });
        assert_eq!(2, config.cells.len());
    }

    #[test]
    fn cell_config_yaml_file() -> anyhow::Result<()> {
        let config_init = CellConfig {
            ..Default::default()
        };

        let dir = tempfile::tempdir()?;
        let file = dir.path().join("file");

        config_init.to_yaml_file(&file)?;

        let config_read = CellConfig::from_yaml_file(&file)?;

        assert_eq!(config_init, config_read);

        Ok(())
    }

    #[test]
    fn cell_config_add_node() {
        let mut config = CellConfig {
            ..Default::default()
        };

        let node1 = CellNodeConfig {
            node: Some(NodeConfig {
                public_key: "pk1".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        config.add_node(node1);
        assert_eq!(config.nodes.len(), 1);

        let node1_changed = CellNodeConfig {
            node: Some(NodeConfig {
                public_key: "pk1".to_string(),
                name: "new name".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        config.add_node(node1_changed);
        assert_eq!(config.nodes.len(), 1);
        assert_eq!("new name", config.nodes[0].node.as_ref().unwrap().name);

        let node2 = CellNodeConfig {
            node: Some(NodeConfig {
                public_key: "pk2".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        config.add_node(node2);
        assert_eq!(config.nodes.len(), 2);
    }

    #[test]
    fn cell_config_add_app() {
        let mut config = CellConfig {
            ..Default::default()
        };

        config.add_application(CellApplicationConfig {
            name: "name".to_string(),
            version: "0.0.1".to_string(),
            public_key: "pk1".to_string(),
            package_url: "https://some/location/package.zip".to_string(),
            location: Some(cell_application_config::Location::Inline(Manifest {
                ..Default::default()
            })),
        });
        assert_eq!(config.apps.len(), 1);

        config.add_application(CellApplicationConfig {
            name: "name".to_string(),
            version: "0.0.1".to_string(),
            public_key: "pk1".to_string(),
            package_url: "https://some/location/package.zip".to_string(),
            location: Some(cell_application_config::Location::Inline(Manifest {
                ..Default::default()
            })),
        });
        assert_eq!(config.apps.len(), 1);

        config.add_application(CellApplicationConfig {
            name: "name".to_string(),
            version: "0.0.1".to_string(),
            public_key: "pk2".to_string(),
            package_url: "https://some/location/package.zip".to_string(),
            location: Some(cell_application_config::Location::Inline(Manifest {
                ..Default::default()
            })),
        });
        assert_eq!(config.apps.len(), 2);
    }
}
