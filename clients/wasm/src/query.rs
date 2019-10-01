use exocore_index::query::Query as IndexQuery;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Query {
    pub(crate) inner: IndexQuery,
}

#[wasm_bindgen]
impl Query {
    #[wasm_bindgen]
    pub fn match_text(query: String) -> Query {
        Query {
            inner: IndexQuery::match_text(query),
        }
    }

    #[wasm_bindgen]
    pub fn with_trait(trait_name: String) -> Query {
        Query {
            inner: IndexQuery::with_trait(trait_name),
        }
    }
}
