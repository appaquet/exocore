use crate::domain::entity::{FieldValue, Record, Trait};
use crate::domain::schema;
use crate::error::Error;
use crate::query::*;
use std::result::Result;
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{IntOptions, Schema as TantivySchema, SchemaBuilder, STORED, TEXT};
use tantivy::{Document, Index, IndexWriter};

pub struct Indexer {
    writer: IndexWriter,
    index: Index,
    schema: Arc<schema::Schema>,
    tantivy_schema: TantivySchema,
}

impl Indexer {
    pub fn for_schema(schema: Arc<schema::Schema>) -> Result<Indexer, Error> {
        let tantivy_schema = Indexer::build_schema(schema.as_ref());
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
        for trt in traits {
            let mut trait_doc = Document::default();
            let record_schema: &schema::TraitSchema = trt.record_schema();

            let indexed_fields = record_schema.fields.iter().filter(|f| f.indexed);
            for field in indexed_fields {
                // TODO: Should probably not unwrap, but just continue
                let schema_field = self.tantivy_schema.get_field(&field.name).unwrap();

                if let Some(field_value) = trt.value(field) {
                    match (&field.typ, field_value) {
                        (schema::FieldType::String, FieldValue::String(v)) => {
                            trait_doc.add_text(schema_field, &v)
                        }
                        (schema::FieldType::Int, FieldValue::Int(v)) => {
                            trait_doc.add_i64(schema_field, *v)
                        }
                        _ => panic!(
                            "Type not supported yet: ({:?}, {:?})",
                            field.typ, field_value
                        ),
                    }
                }
            }

            self.writer.add_document(trait_doc);
        }

        let last_ts = self.writer.commit()?;

        Ok(last_ts)
    }

    pub fn search(&self, query: &dyn OldQuery) -> Result<Vec<String>, Error> {
        // TODO: Should not re-create index reader at every search
        let index_reader = self.index.reader()?;
        let searcher = index_reader.searcher();

        let fields = &query
            .fields()
            .iter()
            .flat_map(|f| self.tantivy_schema.get_field(&f))
            .collect::<Vec<_>>();

        let query_parser = QueryParser::for_index(&self.index, fields.clone());

        let query = query_parser.parse_query(&query.value())?;
        let top_collector = TopDocs::with_limit(10);

        let results = searcher.search(&*query, &top_collector)?;

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

    fn build_schema(schema: &schema::Schema) -> TantivySchema {
        let mut schema_builder = SchemaBuilder::default();

        schema_builder.add_text_field("type", TEXT | STORED);
        schema_builder.add_text_field("trait", TEXT | STORED);
        schema_builder.add_text_field("trait_combined_id", TEXT);
        schema_builder.add_text_field("trait_entity_id", TEXT | STORED);
        schema_builder.add_text_field("trait_id", TEXT | STORED);
        schema_builder.add_text_field("trait_type", TEXT | STORED);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_segment_trait() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let contact_trait = Trait::new(schema.clone(), "contact")
            .with_value_by_name("name", "Justin Trudeau")
            .with_value_by_name("email", "justin.trudeau@gov.ca");
        let traits = vec![contact_trait];

        let mut indexer = Indexer::for_schema(schema)?;
        let last_ts = indexer.index_segment(traits.iter())?;

        assert!(last_ts > 0);

        let q = FieldMatchQuery {
            field_name: "name",
            value: "justin",
        };

        let res = indexer.search(&q)?;
        assert_eq!(res.len(), 1);

        Ok(())
    }

    fn create_test_schema() -> Arc<schema::Schema> {
        Arc::new(
            schema::Schema::parse(
                r#"
        name: contacts
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
        "#,
            )
            .unwrap(),
        )
    }

}
