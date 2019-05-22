use exocore_data;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{IntOptions, Schema, SchemaBuilder, STORED, TEXT};
use tantivy::{Document, Index, IndexWriter};

use crate::entity::*;
use crate::queries::*;
use std::io;
use std::result::Result;

// TODO: Needs to decrypt
/* TODO: Use thread local for decryption + decompression.
        Impose a limit on decrypt and decompress block size...
        https://doc.rust-lang.org/std/macro.thread_local.html
*/

pub struct Indexer {
    writer: IndexWriter,
    index: Index,
    schema: Schema,
}

impl Indexer {
    pub fn for_trait(t: &Trait) -> Result<Indexer, Error> {
        let schema = Indexer::build_schema(t);
        let index = Index::create_from_tempdir(schema.clone())?;
        let writer = index.writer(50_000_000)?;

        Ok(Indexer {
            writer,
            index,
            schema,
        })
    }

    pub fn index_segment<'a, T>(&mut self, traits: T) -> Result<u64, Error>
    where
        T: Iterator<Item = &'a Trait>,
    {
        traits.for_each(|t| {
            let mut trait_doc = Document::default();

            t.fields.iter().for_each(|(field, value)| {
                let schema_field = self.schema.get_field(&field.name).unwrap();

                match (&field.typ, value) {
                    (&FieldType::Text, FieldValue::Text(str)) => {
                        trait_doc.add_text(schema_field, &str)
                    }
                    (&FieldType::Long, FieldValue::Long(lng)) => {
                        trait_doc.add_u64(schema_field, *lng)
                    }
                    _ => panic!("Type not supported yet: ({:?}, {:?})", &field.typ, value),
                }
            });

            self.writer.add_document(trait_doc);
        });

        let last_ts = self.writer.commit()?;

        Ok(last_ts)
    }

    pub fn search(&self, query: &dyn Query) -> Result<Vec<String>, Error> {
        // TODO: Should not re-create index reader at every search
        let index_reader = self.index.reader()?;
        let searcher = index_reader.searcher();

        let fields = &query
            .fields()
            .iter()
            .flat_map(|f| self.schema.get_field(&f))
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

    fn build_schema(t: &Trait) -> Schema {
        let mut schema_builder = SchemaBuilder::default();

        schema_builder.add_text_field("type", TEXT | STORED);
        schema_builder.add_text_field("trait", TEXT | STORED);
        schema_builder.add_text_field("trait_combined_id", TEXT);
        schema_builder.add_text_field("trait_entity_id", TEXT | STORED);
        schema_builder.add_text_field("trait_id", TEXT | STORED);
        schema_builder.add_text_field("trait_type", TEXT | STORED);

        t.fields.iter().for_each(|(field, _)| {
            match field.typ {
                FieldType::Long => {
                    let mut options = IntOptions::default();
                    if field.indexed {
                        options = options.set_indexed();
                    }
                    schema_builder.add_u64_field(&field.name, options);
                }
                FieldType::Text => {
                    // TODO : Allow different text indexing options
                    let mut options = TEXT;
                    if field.stored {
                        options = options.set_stored();
                    }
                    schema_builder.add_text_field(&field.name, options);
                }
                _ => panic!("Type not supported yet"),
            }
        });

        schema_builder.build()
    }
}

pub struct Store<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    _data_handle: exocore_data::engine::EngineHandle<CP, PP>,
}

impl<CP, PP> Store<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    fn start(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

impl From<tantivy::query::QueryParserError> for Error {
    fn from(err: tantivy::query::QueryParserError) -> Self {
        Error::QueryParserError(err)
    }
}

impl From<tantivy::TantivyError> for Error {
    fn from(err: tantivy::TantivyError) -> Self {
        Error::TantivyError(err)
    }
}

impl From<io::Error> for Error {
    fn from(_: io::Error) -> Self {
        Error::FileError
    }
}

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "An error as occurred in Query parsing: {:?}", _0)]
    QueryParserError(tantivy::query::QueryParserError),
    #[fail(display = "An error as occurred in Tantivy: {}", _0)]
    TantivyError(tantivy::TantivyError),
    #[fail(display = "A file error as occurred.")]
    FileError,
    #[fail(display = "An error occurred: {}", _0)]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_segment_trait() -> Result<(), failure::Error> {
        let mut contact_trait = Trait::new(TraitType::Unique, "contact");
        contact_trait.with_field(
            "email",
            FieldType::Text,
            FieldValue::Text("justin.trudeau@gov.ca".to_string()),
            true,
            true,
        );
        contact_trait.with_field(
            "name",
            FieldType::Text,
            FieldValue::Text("Justin Trudeau".to_string()),
            true,
            true,
        );

        let traits = vec![&contact_trait];
        let mut indexer = Indexer::for_trait(&contact_trait)?;
        let last_ts = indexer.index_segment(traits.into_iter())?;

        assert!(last_ts > 0);

        let q = FieldMatchQuery {
            field_name: "email",
            value: "justin.trudeau@gov.ca",
        };

        let res = indexer.search(&q)?;

        assert_eq!(res.len(), 1);

        Ok(())
    }
}
