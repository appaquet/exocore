use crate::domain::entity::{EntityId, FieldValue, Record, Trait, TraitId};
use crate::domain::schema;
use crate::error::Error;
use crate::query::*;
use exocore_data::block::BlockOffset;
use exocore_data::operation::OperationId;
use std::ops::Deref;
use std::result::Result;
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::query::{QueryParser, TermQuery};
use tantivy::schema::{
    IndexRecordOption, IntOptions, Schema as TantivySchema, SchemaBuilder, INDEXED, STORED, TEXT,
};
use tantivy::{Document, Index, IndexWriter, Searcher, Term};

pub struct Indexer {
    writer: IndexWriter,
    index: Index,
    schema: Arc<schema::Schema>,
    tantivy_schema: TantivySchema,
}

impl Indexer {
    pub fn for_schema(schema: Arc<schema::Schema>) -> Result<Indexer, Error> {
        let tantivy_schema = Indexer::build_tantivy_schema(schema.as_ref());
        let index = Index::create_from_tempdir(tantivy_schema.clone())?;
        let writer = index.writer(50_000_000)?;

        Ok(Indexer {
            writer,
            index,
            schema,
            tantivy_schema,
        })
    }

    pub fn index_segment<'a, T>(&mut self, traits: T) -> Result<u64, Error>
    where
        T: Iterator<Item = &'a Trait>,
    {
        let trait_type_field = self.get_tantivy_field("trait_type");
        let text_field = self.get_tantivy_field("text");

        for trt in traits {
            let mut doc = Document::default();
            let record_schema: &schema::TraitSchema = trt.record_schema();

            // TODO: namespace name
            doc.add_u64(trait_type_field, u64::from(record_schema.id));

            let indexed_fields = record_schema.fields.iter().filter(|f| f.indexed);
            for field in indexed_fields {
                // TODO: Should probably not unwrap, but just continue
                let schema_field = self.tantivy_schema.get_field(&field.name).unwrap();

                if let Some(field_value) = trt.value(field) {
                    match (&field.typ, field_value) {
                        (schema::FieldType::String, FieldValue::String(v)) => {
                            doc.add_text(schema_field, &v);
                            doc.add_text(text_field, &v);
                        }
                        (schema::FieldType::Int, FieldValue::Int(v)) => {
                            doc.add_i64(schema_field, *v);
                        }
                        _ => panic!(
                            "Type not supported yet: ({:?}, {:?})",
                            field.typ, field_value
                        ),
                    }
                }
            }

            self.writer.add_document(doc);
        }

        let last_ts = self.writer.commit()?;

        Ok(last_ts)
    }

    pub fn search(&self, query: Query) -> Result<Vec<String>, Error> {
        // TODO: Should not re-create index reader at every search
        let index_reader = self.index.reader()?;
        let searcher = index_reader.searcher();

        let res = match query {
            Query::WithTrait(inner_query) => self.search_with_trait(searcher, inner_query)?,
            Query::Match(inner_query) => self.search_matches(searcher, inner_query)?,
            Query::Empty => vec![],
            // TODO: Query::Conjunction(inner_query) => vec![],
            _other => vec![],
        };

        Ok(res)
    }

    fn search_with_trait<S>(&self, searcher: S, query: WithTraitQuery) -> Result<Vec<String>, Error>
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

    fn search_matches<S>(&self, searcher: S, query: MatchQuery) -> Result<Vec<String>, Error>
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
    ) -> Result<Vec<String>, Error>
    where
        S: Deref<Target = Searcher>,
    {
        let top_collector = TopDocs::with_limit(10);
        let results = searcher.search(query, &top_collector)?;

        let mut res = Vec::new();
        for (score, doc) in results {
            let r = searcher.doc(doc)?;
            res.push(format!(
                "Result for {:?} -> {:?} score {:?}",
                query, r, score
            ));
        }

        Ok(res)
    }

    fn get_tantivy_field(&self, name: &str) -> tantivy::schema::Field {
        self.tantivy_schema
            .get_field(name)
            .unwrap_or_else(|| panic!("Couldn't find {} field in Tantivy schema", name))
    }

    fn build_tantivy_schema(schema: &schema::Schema) -> TantivySchema {
        let mut schema_builder = SchemaBuilder::default();

        schema_builder.add_text_field("type", TEXT | STORED);
        schema_builder.add_text_field("trait", TEXT | STORED);
        schema_builder.add_text_field("trait_combined_id", TEXT);
        schema_builder.add_text_field("trait_entity_id", TEXT | STORED);
        schema_builder.add_text_field("trait_id", TEXT | STORED);

        // TODO: entity id
        // TODO: trait id

        schema_builder.add_u64_field("trait_type", INDEXED | STORED);
        schema_builder.add_text_field("text", TEXT);

        // TODO: This doesn't support evolution of the schema. Tantivy schema is immutable
        for trt in &schema.traits {
            for field in &trt.fields {
                match field.typ {
                    schema::FieldType::Int if field.indexed => {
                        let mut options = IntOptions::default();
                        if field.indexed {
                            options = options.set_indexed();
                        }
                        schema_builder.add_i64_field(&field.name, options);
                    }
                    schema::FieldType::String if field.indexed => {
                        // TODO : Allow different text indexing options
                        let options = TEXT;
                        schema_builder.add_text_field(&field.name, options);
                    }
                    _ => {
                        // no need to be indexed
                    }
                }
            }
        }

        schema_builder.build()
    }
}

///
///
///
pub struct StoredTrait {
    pub block_offset: Option<BlockOffset>,
    pub operation_id: Option<OperationId>,
    pub entity_id: EntityId,
    pub trt: Trait,
}


///
///
///
pub struct TraitResult {
    pub block_offset: Option<BlockOffset>,
    pub operation_id: Option<OperationId>,
    pub entity_id: EntityId,
    pub trait_id: TraitId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_matches() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut indexer = Indexer::for_schema(schema.clone())?;

        let contact_trait = Trait::new(schema.clone(), "contact")
            .with_value_by_name("name", "Justin Trudeau")
            .with_value_by_name("email", "justin.trudeau@gov.ca");
        let traits = vec![contact_trait];
        let last_ts = indexer.index_segment(traits.iter())?;
        assert!(last_ts > 0);

        let query = Query::Match(MatchQuery {
            query: "justin".to_string(),
        });
        let res = indexer.search(query)?;
        assert_eq!(res.len(), 1);

        Ok(())
    }

    #[test]
    fn search_query_by_trait_type() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut indexer = Indexer::for_schema(schema.clone())?;

        let contact_trait = Trait::new(schema.clone(), "contact")
            .with_value_by_name("name", "Justin Trudeau")
            .with_value_by_name("email", "justin.trudeau@gov.ca");
        let email_trait = Trait::new(schema.clone(), "email")
            .with_value_by_name("subject", "Some subject")
            .with_value_by_name("body", "Very important body");
        let traits = vec![contact_trait, email_trait];
        indexer.index_segment(traits.iter())?;

        let results = indexer.search(Query::WithTrait(WithTraitQuery {
            trait_name: "email".to_string(),
            trait_query: None,
        }))?;

        println!("{:?}", results);

        Ok(())
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
