use std::sync::Arc;

use futures::Future;
use wasm_bindgen::__rt::std::collections::HashMap;
use wasm_bindgen::prelude::*;

use exocore_index::mutation::Mutation;
use exocore_index::store::remote::StoreHandle;
use exocore_index::store::AsyncStore;
use exocore_schema::entity::{FieldValue, RecordBuilder, TraitBuilder};
use exocore_schema::schema::Schema;
use exocore_schema::serialization::with_schema;

use crate::js::into_js_error;

#[wasm_bindgen]
pub struct MutationBuilder {
    schema: Arc<Schema>,
    store_handle: Arc<StoreHandle>,

    inner: Option<Mutation>,
}

#[wasm_bindgen]
impl MutationBuilder {
    pub(crate) fn new(schema: Arc<Schema>, store_handle: Arc<StoreHandle>) -> MutationBuilder {
        MutationBuilder {
            schema,
            store_handle,
            inner: None,
        }
    }

    #[wasm_bindgen]
    pub fn put_trait(
        mut self,
        entity_id: String,
        namespace: &str,
        trait_type: &str,
        data: JsValue,
    ) -> MutationBuilder {
        let dict: HashMap<String, FieldValue> = data.into_serde().expect("Couldn't parse data");

        let mut trait_builder = TraitBuilder::new(&self.schema, namespace, trait_type)
            .expect("Couldn't create TraitBuilder");
        for (name, value) in dict {
            trait_builder = trait_builder.set(&name, value);
        }
        let trt = trait_builder.build().expect("Couldn't build trait");

        self.inner = Some(Mutation::put_trait(entity_id, trt));

        self
    }

    #[wasm_bindgen]
    pub fn delete_trait(mut self, entity_id: String, trait_id: String) -> MutationBuilder {
        self.inner = Some(Mutation::delete_trait(entity_id, trait_id));
        self
    }

    #[wasm_bindgen]
    pub fn execute(self) -> js_sys::Promise {
        let mutation = self.inner.expect("Mutation was not initialized");

        let schema = self.schema;
        let fut_result = self
            .store_handle
            .mutate(mutation)
            .map(move |res| {
                with_schema(&schema, || JsValue::from_serde(&res)).unwrap_or_else(into_js_error)
            })
            .map_err(into_js_error);

        wasm_bindgen_futures::future_to_promise(fut_result)
    }
}
