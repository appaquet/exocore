use std::rc::Rc;

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

    pub fn push_node_config(&self, local_node: LocalNode) -> js_sys::Promise {
        let client = self.client.clone();

        let ret = async move {
            let payload = local_node.to_yaml()?; // TODO: Should be proto
            let create_resp = client
                .create(payload.as_bytes())
                .await
                .map_err(into_js_error)?;

            Ok(create_resp.id.to_formatted_string().into())
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
