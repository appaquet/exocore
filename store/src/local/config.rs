#[derive(Clone, Copy, Debug)]
pub struct NodeStoreConfig {
    index: EntityIndexConfig
}

impl Default for NodeStoreConfig {
    fn default() -> Self {
      NodeStoreconfig {
          index: EntityIndexConfig::default(),
      }
    }
}
