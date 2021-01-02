use exocore_core::protos::generated::exocore_core::MutationIndexConfig as ProtoMutationIndexConfig;
/// Trait index configuration
#[derive(Clone, Copy, Debug)]
pub struct MutationIndexConfig {
    pub indexer_num_threads: Option<usize>,
    pub indexer_heap_size_bytes: usize,
    pub iterator_page_size: u32,
    pub iterator_max_pages: usize,
    pub entity_mutations_cache_size: usize,

    pub dynamic_reference_fields: u32,
    pub dynamic_string_fields: u32,
    pub dynamic_text_fields: u32,
    pub dynamic_i64_fields: u32,
    pub dynamic_i64_sortable_fields: u32,
    pub dynamic_u64_fields: u32,
    pub dynamic_u64_sortable_fields: u32,
}

impl Default for MutationIndexConfig {
    fn default() -> Self {
        MutationIndexConfig {
            indexer_num_threads: Some(2),
            indexer_heap_size_bytes: 30_000_000,
            iterator_page_size: 50,
            iterator_max_pages: 50,
            entity_mutations_cache_size: 2000,

            dynamic_reference_fields: 5,
            dynamic_string_fields: 5,
            dynamic_text_fields: 5,
            dynamic_i64_fields: 2,
            dynamic_i64_sortable_fields: 2,
            dynamic_u64_fields: 2,
            dynamic_u64_sortable_fields: 2,
        }
    }
}

impl From<ProtoMutationIndexConfig> for MutationIndexConfig {
    fn from(proto: ProtoMutationIndexConfig) -> Self {
        let mut config = MutationIndexConfig {
            indexer_num_threads: Some(proto.indexer_num_threads as usize).filter(|&n| n == 0),
            ..MutationIndexConfig::default()
        };

        if proto.indexer_heap_size_bytes > 0 {
            config.indexer_heap_size_bytes = proto.indexer_heap_size_bytes as usize;
        }

        if proto.iterator_page_size > 0 {
            config.iterator_page_size = proto.iterator_page_size;
        }

        if proto.iterator_max_pages > 0 {
            config.iterator_max_pages = proto.iterator_max_pages as usize;
        }

        if proto.entity_mutations_cache_size > 0 {
            config.entity_mutations_cache_size = proto.entity_mutations_cache_size as usize;
        }

        config
    }
}
