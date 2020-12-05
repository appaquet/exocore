use std::{rc::Rc, time::Duration};

use exocore_core::{
    cell::{CellConfigExt, LocalNodeConfigExt},
    protos::core::{node_cell_config, CellConfig, NodeCellConfig},
};
use exocore_discovery::{Client, DEFAULT_DISCO_SERVER};
use wasm_bindgen::prelude::*;

use crate::{js::into_js_error, node::LocalNode};
#[wasm_bindgen]
pub struct Discovery {
    client: Rc<Client>,
}

#[wasm_bindgen]
impl Discovery {
    pub fn new(disco_url: Option<String>) -> Discovery {
        let disco_url = disco_url.as_deref().unwrap_or(DEFAULT_DISCO_SERVER);
        let client = Client::new(disco_url).expect("Couldn't create client");

        Discovery {
            client: Rc::new(client),
        }
    }

    pub fn push_node_config(
        &self,
        local_node: LocalNode,
        pin_callback: js_sys::Function,
    ) -> js_sys::Promise {
        let client = self.client.clone();

        let ret = async move {
            let payload = local_node.to_yaml()?; // TODO: Should be proto
            let create_resp = client
                .create(payload.as_bytes(), true)
                .await
                .map_err(into_js_error)?;

            let pin = create_resp.pin.to_formatted_string();
            pin_callback
                .call1(&JsValue::null(), &JsValue::from_str(pin.as_str()))
                .expect("Error calling pin callback");

            let reply_pin = create_resp
                .reply_pin
                .expect("Expected reply pin on created payload");
            let get_cell_resp = client
                .get_loop(reply_pin, Duration::from_secs(60))
                .await
                .map_err(into_js_error)?;

            let get_cell_payload = get_cell_resp
                .decode_payload()
                .expect("Couldn't decode payload");
            let cell_config = CellConfig::from_yaml(get_cell_payload.as_slice())
                .expect("Couldn't decode cell config");

            let mut local_node_config = local_node.config.clone();
            local_node_config.add_cell(NodeCellConfig {
                location: Some(node_cell_config::Location::Inline(cell_config)),
            });

            let local_node = LocalNode::from_config(local_node_config)
                .expect("Couldn't create LocalNode from config");
            Ok(local_node.into())
        };

        wasm_bindgen_futures::future_to_promise(ret)
    }

    // pub fn join_cell(&self, local_node: LocalNode, pin: String) -> js_sys::Promise {
    //     let client = self.client.clone();

    //     let ret = async move {
    //         // let parsed_pin = Pin::from()

    //         let payload = local_node.to_yaml()?; // TODO: Should be proto
    //         let create_resp = client
    //             .create(payload.as_bytes())
    //             .await
    //             .map_err(into_js_error)?;

    //         Ok(create_resp.id.to_formatted_string().into())
    //     };

    //     wasm_bindgen_futures::future_to_promise(ret)
    // }
}
