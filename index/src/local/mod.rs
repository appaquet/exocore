mod entity_index;
mod mutation_index;
mod store;
mod top_results;
mod tracked_mutations;
mod watched_queries;

#[cfg(feature = "local_store")]
pub use entity_index::{EntityIndex, EntityIndexConfig};
pub use store::{Store, StoreHandle};

#[cfg(test)]
mod test_store;

#[cfg(test)]
pub use test_store::TestStore;
