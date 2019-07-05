use crate::domain::entity::{EntityId, FieldValue, Record, Trait, TraitId};
use crate::domain::schema;
use crate::error::Error;
use crate::query::*;
use exocore_data::block::BlockOffset;
use exocore_data::operation::OperationId;
use std::ops::Deref;
use std::path::Path;
use std::result::Result;
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{QueryParser, TermQuery};
use tantivy::schema::{
    IndexRecordOption, Schema as TantivySchema, SchemaBuilder, INDEXED, STORED, STRING, TEXT,
};
use tantivy::{Document, Index as TantivyIndex, IndexReader, Searcher, Term};

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
/// Index (full-text & fields) for traits in the given schema. Each trait is individually
/// indexed as a single document.
///
pub struct TraitsIndex {
    index: TantivyIndex,
    index_reader: IndexReader,
    schema: Arc<schema::Schema>,
    tantivy_schema: TantivySchema,
}

impl TraitsIndex {
    pub fn new_mmap(schema: Arc<schema::Schema>, directory: &Path) -> Result<TraitsIndex, Error> {
        let tantivy_schema = Self::build_tantivy_schema(schema.as_ref());
        let directory = MmapDirectory::open(directory)?;
        let index = TantivyIndex::open_or_create(directory, tantivy_schema.clone())?;
        let index_reader = index.reader()?;

        Ok(TraitsIndex {
            index,
            index_reader,
            schema,
            tantivy_schema,
        })
    }

    pub fn new_in_memory(schema: Arc<schema::Schema>) -> Result<TraitsIndex, Error> {
        let tantivy_schema = Self::build_tantivy_schema(schema.as_ref());
        let index = TantivyIndex::create_in_ram(tantivy_schema.clone());
        let index_reader = index.reader()?;

        Ok(TraitsIndex {
            index,
            index_reader,
            schema,
            tantivy_schema,
        })
    }

    pub fn index_traits<'a, T>(&mut self, stored_traits: T) -> Result<u64, Error>
    where
        T: Iterator<Item = &'a StoredTrait<'a>>,
    {
        let trait_type_field = self.get_tantivy_field("trait_type");
        let trait_id_field = self.get_tantivy_field("trait_id");
        let entity_id_field = self.get_tantivy_field("entity_id");
        let block_offset_field = self.get_tantivy_field("block_offset");
        let operation_id_field = self.get_tantivy_field("operation_id");
        let text_field = self.get_tantivy_field("text");

        let mut index_writer = self.index.writer(50_000_000)?;
        for stored_trait in stored_traits {
            let mut doc = Document::default();
            let record_schema: &schema::TraitSchema = stored_trait.trt.record_schema();

            // TODO: namespace name
            doc.add_u64(trait_type_field, u64::from(record_schema.id));
            doc.add_text(trait_id_field, &stored_trait.trt.id());
            doc.add_text(entity_id_field, &stored_trait.entity_id);
            if let Some(block_offset) = stored_trait.block_offset {
                doc.add_u64(block_offset_field, block_offset);
            }
            if let Some(operation_id) = stored_trait.operation_id {
                doc.add_u64(operation_id_field, operation_id);
            }

            let indexed_fields = record_schema.fields.iter().filter(|f| f.indexed);
            for field in indexed_fields {
                if let Some(field_value) = stored_trait.trt.value(field) {
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

            index_writer.add_document(doc);
        }

        let last_ts = index_writer.commit()?;

        // it make takes milliseconds for reader to see changes automatically, so we force it
        self.index_reader.reload()?;

        Ok(last_ts)
    }

    pub fn search(&self, query: Query) -> Result<Vec<TraitResult>, Error> {
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
        query: WithTraitQuery,
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

    fn search_matches<S>(&self, searcher: S, query: MatchQuery) -> Result<Vec<TraitResult>, Error>
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
        let top_collector = TopDocs::with_limit(10);
        let search_results = searcher.search(query, &top_collector)?;

        let block_offset_field = self.get_tantivy_field("block_offset");
        let operation_id_field = self.get_tantivy_field("operation_id");
        let entity_id_field = self.get_tantivy_field("entity_id");
        let trait_id_field = self.get_tantivy_field("trait_id");

        let mut results = Vec::new();
        for (score, doc_addr) in search_results {
            let doc = searcher.doc(doc_addr)?;
            let block_offset = self.get_tantivy_doc_opt_u64_field(&doc, block_offset_field);
            let operation_id = self.get_tantivy_doc_opt_u64_field(&doc, operation_id_field);
            let entity_id = self.get_tantivy_doc_string_field(&doc, entity_id_field);
            let trait_id = self.get_tantivy_doc_string_field(&doc, trait_id_field);
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

    fn build_tantivy_schema(_schema: &schema::Schema) -> TantivySchema {
        let mut schema_builder = SchemaBuilder::default();

        schema_builder.add_u64_field("trait_type", INDEXED | STORED);
        schema_builder.add_text_field("entity_id", STRING | STORED);
        schema_builder.add_text_field("trait_id", STRING | STORED);
        schema_builder.add_u64_field("block_offset", INDEXED | STORED);
        schema_builder.add_u64_field("operation_id", INDEXED | STORED);

        schema_builder.add_text_field("text", TEXT);

        schema_builder.build()
    }
}

///
/// Trait to be indexed that is stored in the data layer (pending or chain)
///
pub struct StoredTrait<'t> {
    pub block_offset: Option<BlockOffset>,
    pub operation_id: Option<OperationId>,
    pub entity_id: EntityId,
    pub trt: &'t Trait,
}

///
/// Indexed trait returned as a result of a query
///
#[derive(Debug)]
pub struct TraitResult {
    pub block_offset: Option<BlockOffset>,
    pub operation_id: Option<OperationId>,
    pub entity_id: EntityId,
    pub trait_id: TraitId,
    pub score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_matches() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut indexer = TraitsIndex::new_in_memory(schema.clone())?;

        let contact_trait = Trait::new(schema.clone(), "contact")
            .with_id("trudeau1".to_string())
            .with_value_by_name("name", "Justin Trudeau")
            .with_value_by_name("email", "justin.trudeau@gov.ca");
        let contact_stored_trait = StoredTrait {
            block_offset: Some(1234),
            operation_id: Some(2345),
            entity_id: "entity_id1".to_string(),
            trt: &contact_trait,
        };

        let traits = vec![contact_stored_trait];
        let last_ts = indexer.index_traits(traits.iter())?;
        assert!(last_ts > 0);

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        let results = indexer.search(query)?;
        assert_eq!(results.len(), 1);

        let result = find_trait_result(&results, "trudeau1").unwrap();
        assert_eq!(result.block_offset, Some(1234));
        assert_eq!(result.operation_id, Some(2345));
        assert_eq!(result.entity_id, "entity_id1");
        assert_eq!(result.trait_id, "trudeau1");

        Ok(())
    }

    #[test]
    fn search_query_by_trait_type() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut indexer = TraitsIndex::new_in_memory(schema.clone())?;

        let contact_trait = Trait::new(schema.clone(), "contact")
            .with_id("trt1".to_string())
            .with_value_by_name("name", "Justin Trudeau")
            .with_value_by_name("email", "justin.trudeau@gov.ca");
        let contact_stored_trait = StoredTrait {
            block_offset: None,
            operation_id: None,
            entity_id: "entity_id1".to_string(),
            trt: &contact_trait,
        };

        let email_trait = Trait::new(schema.clone(), "email")
            .with_id("trt2".to_string())
            .with_value_by_name("subject", "Some subject")
            .with_value_by_name("body", "Very important body");
        let email_stored_trait = StoredTrait {
            block_offset: None,
            operation_id: None,
            entity_id: "entity_id2".to_string(),
            trt: &email_trait,
        };

        let traits = vec![contact_stored_trait, email_stored_trait];
        indexer.index_traits(traits.iter())?;

        let results = indexer.search(Query::WithTrait(WithTraitQuery {
            trait_name: "email".to_string(),
            trait_query: None,
        }))?;
        assert!(find_trait_result(&results, "trt2").is_some());

        Ok(())
    }

    fn find_trait_result<'r>(
        results: &'r Vec<TraitResult>,
        trait_id: &str,
    ) -> Option<&'r TraitResult> {
        results.iter().find(|t| t.trait_id == trait_id)
    }

    fn create_test_schema() -> Arc<schema::Schema> {
        Arc::new(
            schema::Schema::parse(
                r#"
        name: myschema
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

}
