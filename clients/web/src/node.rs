use exocore_core::{
    cell::{LocalNode as CoreLocalNode, LocalNodeConfigExt},
    protos::core::LocalNodeConfig,
};
use wasm_bindgen::prelude::*;

use crate::js::into_js_error;
#[wasm_bindgen]
pub struct LocalNode {
    _node: CoreLocalNode,
    pub(crate) config: LocalNodeConfig,
}

#[wasm_bindgen]
impl LocalNode {
    pub fn generate() -> LocalNode {
        let node = CoreLocalNode::generate();
        LocalNode {
            _node: node.clone(),
            config: LocalNodeConfig {
                keypair: node.keypair().encode_base58_string(),
                public_key: node.public_key().encode_base58_string(),
                name: node.name().to_string(),
                id: node.id().to_string(),
                ..Default::default()
            },
        }
    }

    pub(crate) fn from_config(config: LocalNodeConfig) -> Result<LocalNode, JsValue> {
        let node = CoreLocalNode::new_from_config(config.clone()).map_err(into_js_error)?;

        Ok(LocalNode {
            _node: node,
            config,
        })
    }

    pub fn from_json(json: String) -> Result<LocalNode, JsValue> {
        let config = LocalNodeConfig::from_json_reader(json.as_bytes()).map_err(into_js_error)?;
        Self::from_config(config)
    }

    pub fn from_yaml(yaml: String) -> Result<LocalNode, JsValue> {
        let config = LocalNodeConfig::from_yaml_reader(yaml.as_bytes()).map_err(into_js_error)?;
        Self::from_config(config)
    }

    pub fn from_storage(&self, storage: web_sys::Storage) -> Result<LocalNode, JsValue> {
        let config_str: Option<String> = storage.get("node_config")?;
        let config = config_str.ok_or("couldn't find `node_config` in storage")?;
        Self::from_json(config)
    }

    pub fn save_to_storage(&self, storage: web_sys::Storage) -> Result<(), JsValue> {
        let config = self.config.inlined().map_err(into_js_error)?;
        let config_json = config.to_json().map_err(into_js_error)?;
        storage.set("node_config", config_json.as_str())?;
        Ok(())
    }

    pub fn to_yaml(&self) -> Result<String, JsValue> {
        self.config.to_yaml().map_err(into_js_error)
    }
}
