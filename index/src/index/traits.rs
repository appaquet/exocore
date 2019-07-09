use crate::domain::entity::{EntityId, FieldValue, Record, Trait, TraitId};
use crate::domain::schema;
use crate::error::Error;
use crate::query::*;
use exocore_data::block::BlockOffset;
use exocore_data::operation::OperationId;
use std::ops::Deref;
use std::path::Path;
use std::result::Result;
use std::sync::{Arc, Mutex};
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{AllQuery, QueryParser, TermQuery};
use tantivy::schema::{
    IndexRecordOption, Schema as TantivySchema, SchemaBuilder, FAST, INDEXED, STORED, STRING, TEXT,
};
use tantivy::{Document, Index as TantivyIndex, IndexReader, IndexWriter, Searcher, Term};

/* TODO: Queries to support
        summary = true or false
        where entity_id = X
        where entity_id IN (x,y,z)
        where entity has trait of type X
        where entity has trait Child refering parent X
        where entity has trait OldChild refering parent X
        where entity matches text XYZ
        where entity with trait of type Postponed and untilDate <= / == / >= someTime
        all()
        limit XYZ
        from page XYZ (nextPage)
*/

///
///
///
#[derive(Clone, Copy, Debug)]
pub struct TraitsIndexConfig {
    pub indexer_num_threads: Option<usize>,
    pub indexer_heap_size_bytes: usize,
}

impl Default for TraitsIndexConfig {
    fn default() -> Self {
        TraitsIndexConfig {
            indexer_num_threads: None,
            indexer_heap_size_bytes: 50_000_000,
        }
    }
}

///
/// Index (full-text & fields) for traits in the given schema. Each trait is individually
/// indexed as a single document.
///
pub struct TraitsIndex {
    index: TantivyIndex,
    index_reader: IndexReader,
    index_writer: Mutex<IndexWriter>,
    schema: Arc<schema::Schema>,
    tantivy_schema: TantivySchema,
}

impl TraitsIndex {
    pub fn open_or_create_mmap(
        config: TraitsIndexConfig,
        schema: Arc<schema::Schema>,
        directory: &Path,
    ) -> Result<TraitsIndex, Error> {
        let tantivy_schema = Self::build_tantivy_schema(schema.as_ref());
        let directory = MmapDirectory::open(directory)?;
        let index = TantivyIndex::open_or_create(directory, tantivy_schema.clone())?;
        let index_reader = index.reader()?;
        let index_writer = if let Some(nb_threads) = config.indexer_num_threads {
            index.writer_with_num_threads(nb_threads, config.indexer_heap_size_bytes)?
        } else {
            index.writer(config.indexer_heap_size_bytes)?
        };

        Ok(TraitsIndex {
            index,
            index_reader,
            index_writer: Mutex::new(index_writer),
            schema,
            tantivy_schema,
        })
    }

    pub fn create_in_memory(
        config: TraitsIndexConfig,
        schema: Arc<schema::Schema>,
    ) -> Result<TraitsIndex, Error> {
        let tantivy_schema = Self::build_tantivy_schema(schema.as_ref());
        let index = TantivyIndex::create_in_ram(tantivy_schema.clone());
        let index_reader = index.reader()?;
        let index_writer = if let Some(nb_threads) = config.indexer_num_threads {
            index.writer_with_num_threads(nb_threads, config.indexer_heap_size_bytes)?
        } else {
            index.writer(config.indexer_heap_size_bytes)?
        };

        Ok(TraitsIndex {
            index,
            index_reader,
            index_writer: Mutex::new(index_writer),
            schema,
            tantivy_schema,
        })
    }

    pub fn apply_mutation(&mut self, mutation: IndexMutation) -> Result<(), Error> {
        self.apply_mutations(Some(mutation).into_iter())
    }

    pub fn apply_mutations<T>(&mut self, mutations: T) -> Result<(), Error>
    where
        T: Iterator<Item = IndexMutation>,
    {
        let mut index_writer = self.index_writer.lock().unwrap();

        debug!("Starting applying mutations to index...");
        let entity_trait_id_field = self.get_tantivy_field("entity_trait_id");
        let operation_id_field = self.get_tantivy_field("operation_id");
        let mut nb_mutations = 0;
        for mutation in mutations {
            nb_mutations += 1;

            match mutation {
                IndexMutation::PutTrait(new_trait) => {
                    println!("Adding op {}", new_trait.operation_id);
                    let doc = self.trait_to_tantivy_document(&new_trait);
                    index_writer.add_document(doc);
                }
                IndexMutation::DeleteTrait(entity_id, trait_id) => {
                    let entity_trait_id = format!("{}_{}", entity_id, trait_id);
                    index_writer.delete_term(Term::from_field_text(
                        entity_trait_id_field,
                        &entity_trait_id,
                    ));
                }
                IndexMutation::DeleteOperation(operation_id) => {
                    index_writer
                        .delete_term(Term::from_field_u64(operation_id_field, operation_id));
                }
            }
        }

        if nb_mutations > 0 {
            debug!("Applied {} mutations, now committing", nb_mutations);

            // it make takes milliseconds for reader to see committed changes, so we force reload
            index_writer.commit()?;

            self.index_reader.reload()?;
        } else {
            debug!("Applied 0 mutations, not committing");
        }

        Ok(())
    }

    fn trait_to_tantivy_document(&self, trait_mutation: &PutTraitMutation) -> Document {
        // TODO: extract those fields once
        let trait_type_field = self.get_tantivy_field("trait_type");
        let trait_id_field = self.get_tantivy_field("trait_id");
        let entity_id_field = self.get_tantivy_field("entity_id");
        let entity_trait_id_field = self.get_tantivy_field("entity_trait_id");
        let block_offset_field = self.get_tantivy_field("block_offset");
        let operation_id_field = self.get_tantivy_field("operation_id");
        let text_field = self.get_tantivy_field("text");

        // TODO: namespace name
        let record_schema: &schema::TraitSchema = trait_mutation.trt.record_schema();

        let mut doc = Document::default();
        doc.add_u64(trait_type_field, u64::from(record_schema.id));
        doc.add_text(trait_id_field, &trait_mutation.trt.id());
        doc.add_text(entity_id_field, &trait_mutation.entity_id);
        doc.add_text(
            entity_trait_id_field,
            &format!("{}_{}", trait_mutation.entity_id, trait_mutation.trt.id()),
        );
        doc.add_u64(operation_id_field, trait_mutation.operation_id);

        if let Some(block_offset) = trait_mutation.block_offset {
            doc.add_u64(block_offset_field, block_offset);
        }

        let indexed_fields = record_schema.fields.iter().filter(|f| f.indexed);
        for field in indexed_fields {
            if let Some(field_value) = trait_mutation.trt.value(field) {
                match (&field.typ, field_value) {
                    (schema::FieldType::String, FieldValue::String(v)) => {
                        doc.add_text(text_field, &v);
                    }
                    _ => panic!(
                        "Type not supported yet: ({:?}, {:?})",
                        field.typ, field_value
                    ),
                }
            }
        }

        doc
    }

    pub fn highest_indexed_block(&self) -> Result<Option<BlockOffset>, Error> {
        let searcher = self.index_reader.searcher();

        let block_offset_field = self.get_tantivy_field("block_offset");

        let query = AllQuery;
        let top_collector = TopDocs::with_limit(1).order_by_field::<u64>(block_offset_field);
        let search_results = searcher.search(&query, &top_collector)?;

        Ok(search_results
            .first()
            .map(|(block_offset, _doc_addr)| *block_offset))
    }

    pub fn search(&self, query: &Query) -> Result<Vec<TraitResult>, Error> {
        let searcher = self.index_reader.searcher();
        let res = match query {
            Query::WithTrait(inner_query) => self.search_with_trait(searcher, inner_query)?,
            Query::Match(inner_query) => self.search_matches(searcher, inner_query)?,
            Query::Empty => vec![],
            Query::Conjunction(_inner_query) => unimplemented!(),
        };

        Ok(res)
    }

    fn search_with_trait<S>(
        &self,
        searcher: S,
        query: &WithTraitQuery,
    ) -> Result<Vec<TraitResult>, Error>
    where
        S: Deref<Target = Searcher>,
    {
        let trait_schema = if let Some(trait_schema) = self.schema.trait_by_name(&query.trait_name)
        {
            trait_schema
        } else {
            return Ok(vec![]);
        };

        let trait_type_field = self.get_tantivy_field("trait_type");
        let term = Term::from_field_u64(trait_type_field, u64::from(trait_schema.id));
        let query = TermQuery::new(term, IndexRecordOption::Basic);

        self.execute_tantivy_query(searcher, &query)
    }

    fn search_matches<S>(&self, searcher: S, query: &MatchQuery) -> Result<Vec<TraitResult>, Error>
    where
        S: Deref<Target = Searcher>,
    {
        let text_field = self.get_tantivy_field("text");
        let query_parser = QueryParser::for_index(&self.index, vec![text_field]);
        let query = query_parser.parse_query(&query.query)?;

        self.execute_tantivy_query(searcher, &query)
    }

    fn execute_tantivy_query<S>(
        &self,
        searcher: S,
        query: &dyn tantivy::query::Query,
    ) -> Result<Vec<TraitResult>, Error>
    where
        S: Deref<Target = Searcher>,
    {
        // TODO: count should come from query
        let top_collector = TopDocs::with_limit(50);
        let search_results = searcher.search(query, &top_collector)?;

        let block_offset_field = self.get_tantivy_field("block_offset");
        let operation_id_field = self.get_tantivy_field("operation_id");
        let entity_id_field = self.get_tantivy_field("entity_id");
        let trait_id_field = self.get_tantivy_field("trait_id");

        let mut results = Vec::new();
        for (score, doc_addr) in search_results {
            let doc = searcher.doc(doc_addr)?;
            let block_offset = self.get_tantivy_doc_opt_u64_field(&doc, block_offset_field);
            let operation_id = self.get_tantivy_doc_u64_field(&doc, operation_id_field);
            let entity_id = self.get_tantivy_doc_string_field(&doc, entity_id_field);
            let trait_id = self.get_tantivy_doc_string_field(&doc, trait_id_field);
            println!("Result {}", operation_id);
            let result = TraitResult {
                block_offset,
                operation_id,
                entity_id,
                trait_id,
                score,
            };
            results.push(result);
        }

        Ok(results)
    }

    fn get_tantivy_field(&self, name: &str) -> tantivy::schema::Field {
        self.tantivy_schema
            .get_field(name)
            .unwrap_or_else(|| panic!("Couldn't find {} field in Tantivy schema", name))
    }

    fn get_tantivy_doc_string_field(
        &self,
        doc: &Document,
        field: tantivy::schema::Field,
    ) -> String {
        match doc.get_first(field) {
            Some(tantivy::schema::Value::Str(v)) => v.to_string(),
            _ => panic!("Couldn't find field of type string"),
        }
    }

    fn get_tantivy_doc_opt_u64_field(
        &self,
        doc: &Document,
        field: tantivy::schema::Field,
    ) -> Option<u64> {
        match doc.get_first(field) {
            Some(tantivy::schema::Value::U64(v)) => Some(*v),
            _ => None,
        }
    }

    fn get_tantivy_doc_u64_field(&self, doc: &Document, field: tantivy::schema::Field) -> u64 {
        match doc.get_first(field) {
            Some(tantivy::schema::Value::U64(v)) => *v,
            _ => panic!("Couldn't find field of type u64"),
        }
    }

    fn build_tantivy_schema(_schema: &schema::Schema) -> TantivySchema {
        let mut schema_builder = SchemaBuilder::default();

        schema_builder.add_u64_field("trait_type", INDEXED | STORED);
        schema_builder.add_text_field("entity_id", STRING | STORED);
        schema_builder.add_text_field("trait_id", STRING | STORED);
        schema_builder.add_text_field("entity_trait_id", STRING);
        schema_builder.add_u64_field("block_offset", INDEXED | STORED | FAST);
        schema_builder.add_u64_field("operation_id", INDEXED | STORED);

        schema_builder.add_text_field("text", TEXT);

        schema_builder.build()
    }
}

///
/// Mutation to applied to the index
///
pub enum IndexMutation {
    PutTrait(PutTraitMutation),
    DeleteTrait(EntityId, TraitId),
    DeleteOperation(OperationId),
}

pub struct PutTraitMutation {
    pub block_offset: Option<BlockOffset>,
    pub operation_id: OperationId,
    pub entity_id: EntityId,
    pub trt: Trait,
}

///
/// Indexed trait returned as a result of a query
///
#[derive(Debug)]
pub struct TraitResult {
    pub block_offset: Option<BlockOffset>,
    pub operation_id: OperationId,
    pub entity_id: EntityId,
    pub trait_id: TraitId,
    pub score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::schema::tests::create_test_schema;

    #[test]
    fn search_query_matches() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let config = TraitsIndexConfig::default();
        let mut indexer = TraitsIndex::create_in_memory(config, schema.clone())?;

        let contact_mutation = IndexMutation::PutTrait(PutTraitMutation {
            block_offset: Some(1234),
            operation_id: 2345,
            entity_id: "entity_id1".to_string(),
            trt: Trait::new(schema.clone(), "contact")
                .with_id("trudeau1".to_string())
                .with_value_by_name("name", "Justin Trudeau")
                .with_value_by_name("email", "justin.trudeau@gov.ca"),
        });

        indexer.apply_mutation(contact_mutation)?;

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        let results = indexer.search(&query)?;
        assert_eq!(results.len(), 1);

        let result = find_trait_result(&results, "trudeau1").unwrap();
        assert_eq!(result.block_offset, Some(1234));
        assert_eq!(result.operation_id, 2345);
        assert_eq!(result.entity_id, "entity_id1");
        assert_eq!(result.trait_id, "trudeau1");

        Ok(())
    }

    #[test]
    fn search_query_by_trait_type() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let config = TraitsIndexConfig::default();
        let mut index = TraitsIndex::create_in_memory(config, schema.clone())?;

        let contact_mutation = IndexMutation::PutTrait(PutTraitMutation {
            block_offset: None,
            operation_id: 1,
            entity_id: "entity_id1".to_string(),
            trt: Trait::new(schema.clone(), "contact")
                .with_id("trt1".to_string())
                .with_value_by_name("name", "Justin Trudeau")
                .with_value_by_name("email", "justin.trudeau@gov.ca"),
        });

        let email_mutation = IndexMutation::PutTrait(PutTraitMutation {
            block_offset: None,
            operation_id: 2,
            entity_id: "entity_id2".to_string(),
            trt: Trait::new(schema.clone(), "email")
                .with_id("trt2".to_string())
                .with_value_by_name("subject", "Some subject")
                .with_value_by_name("body", "Very important body"),
        });

        index.apply_mutations(vec![contact_mutation, email_mutation].into_iter())?;

        let results = index.search(&Query::WithTrait(WithTraitQuery {
            trait_name: "email".to_string(),
            trait_query: None,
        }))?;
        assert!(find_trait_result(&results, "trt2").is_some());

        Ok(())
    }

    #[test]
    fn highest_indexed_block() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let config = TraitsIndexConfig::default();
        let mut index = TraitsIndex::create_in_memory(config, schema.clone())?;

        assert_eq!(index.highest_indexed_block()?, None);

        index.apply_mutation(IndexMutation::PutTrait(PutTraitMutation {
            block_offset: Some(1234),
            operation_id: 1,
            entity_id: "et1".to_string(),
            trt: Trait::new(schema.clone(), "contact").with_id("trt1".to_string()),
        }))?;
        assert_eq!(index.highest_indexed_block()?, Some(1234));

        index.apply_mutation(IndexMutation::PutTrait(PutTraitMutation {
            block_offset: Some(120),
            operation_id: 2,
            entity_id: "et1".to_string(),
            trt: Trait::new(schema.clone(), "contact").with_id("trt1".to_string()),
        }))?;
        assert_eq!(index.highest_indexed_block()?, Some(1234));

        index.apply_mutation(IndexMutation::PutTrait(PutTraitMutation {
            block_offset: Some(9999),
            operation_id: 3,
            entity_id: "et1".to_string(),
            trt: Trait::new(schema.clone(), "contact").with_id("trt1".to_string()),
        }))?;
        assert_eq!(index.highest_indexed_block()?, Some(9999));

        Ok(())
    }

    #[test]
    fn delete_trait_mutation() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let config = TraitsIndexConfig::default();
        let mut index = TraitsIndex::create_in_memory(config, schema.clone())?;

        let contact_mutation = IndexMutation::PutTrait(PutTraitMutation {
            block_offset: None,
            operation_id: 1234,
            entity_id: "entity_id1".to_string(),
            trt: Trait::new(schema.clone(), "contact")
                .with_id("trudeau1".to_string())
                .with_value_by_name("name", "Justin Trudeau")
                .with_value_by_name("email", "justin.trudeau@gov.ca"),
        });
        index.apply_mutation(contact_mutation)?;

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        assert_eq!(index.search(&query)?.len(), 1);

        index.apply_mutation(IndexMutation::DeleteTrait(
            "entity_id1".to_string(),
            "trudeau1".to_string(),
        ))?;

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        assert_eq!(index.search(&query)?.len(), 0);

        Ok(())
    }

    #[test]
    fn delete_operation_id_mutation() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let config = TraitsIndexConfig::default();
        let mut index = TraitsIndex::create_in_memory(config, schema.clone())?;

        let contact_mutation = IndexMutation::PutTrait(PutTraitMutation {
            block_offset: None,
            operation_id: 1234,
            entity_id: "entity_id1".to_string(),
            trt: Trait::new(schema.clone(), "contact")
                .with_id("trudeau1".to_string())
                .with_value_by_name("name", "Justin Trudeau")
                .with_value_by_name("email", "justin.trudeau@gov.ca"),
        });
        index.apply_mutation(contact_mutation)?;

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        assert_eq!(index.search(&query)?.len(), 1);

        index.apply_mutation(IndexMutation::DeleteOperation(1234))?;

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        assert_eq!(index.search(&query)?.len(), 0);

        Ok(())
    }

    #[test]
    fn total_matching_count() -> Result<(), failure::Error> {
        // TODO:

        Ok(())
    }

    fn find_trait_result<'r>(
        results: &'r Vec<TraitResult>,
        trait_id: &str,
    ) -> Option<&'r TraitResult> {
        results.iter().find(|t| t.trait_id == trait_id)
    }

}
