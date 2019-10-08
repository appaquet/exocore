use std::fmt::Display;
use std::sync::Arc;

use futures::Future;
use wasm_bindgen::JsValue;

use exocore_schema::schema::Schema;

pub fn into_js_error<E: Display>(err: E) -> JsValue {
    let js_error = js_sys::Error::new(&format!("Error executing query: {}", err));
    JsValue::from(js_error)
}

// TODO: To be moved https://github.com/appaquet/exocore/issues/123
pub fn js_future_spawner(future: Box<dyn Future<Item = (), Error = ()> + Send>) {
    wasm_bindgen_futures::spawn_local(future);
}

// TODO: To be cleaned up in https://github.com/appaquet/exocore/issues/104
pub fn create_test_schema() -> Arc<Schema> {
    Arc::new(
        Schema::parse(
            r#"
        namespaces:
            - name: exocore
              traits:
                - id: 0
                  name: contact
                  fields:
                    - id: 0
                      name: name
                      type: string
                      indexed: true
                    - id: 1
                      name: email
                      type: string
                      indexed: true
                - id: 1
                  name: email
                  fields:
                    - id: 0
                      name: subject
                      type: string
                      indexed: true
                    - id: 1
                      name: body
                      type: string
                      indexed: true
        "#,
        )
        .unwrap(),
    )
}
