use exocore_data;

use tantivy::{ Document, Index, IndexWriter };
use tantivy::schema::{Schema, SchemaBuilder, IntOptions, TEXT, STORED};

use std::io;
use std::collections::HashMap;
use std::result::Result;
use crate::entity::*;


// TODO: Needs to decrypt
/* TODO: Use thread local for decryption + decompression.
        Impose a limit on decrypt and decompress block size...
        https://doc.rust-lang.org/std/macro.thread_local.html
*/

pub struct Store<T, CP, PP>
where
    T: exocore_transport::TransportHandle,
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    _data_handle: exocore_data::engine::Handle<CP, PP>,
    trait_schemas: HashMap<String, Schema>,
}

impl<T, CP, PP> Store<T, CP, PP>
where
    T: exocore_transport::TransportHandle,
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    fn start(&mut self) -> Result<(), Error> {
        /* BOOTSTRAP */
        let mut contact_trait = Trait::new(TraitType::Unique, "contact");
        contact_trait.with_field("email", FieldType::Text, FieldValue::Text("justin.trudeau@gov.ca".to_string()), true, true);
        contact_trait.with_field("name", FieldType::Text, FieldValue::Text("Justin Trudeau".to_string()), true, true);

        let traits = vec!(&contact_trait);
        /* END BOOTSTRAP */

        let index = Index::create_from_tempdir(self.build_schema(&contact_trait).clone())?;
        let mut index_writer = index.writer(50_000_000)?;

        // TODO : Validate iter() vs into_iter() - Does item needs to be consumed here (IE: into_iter())
        self.index_segment(&index_writer, traits.into_iter())
    }

    pub fn index_segment<'a, T>(&self, writer: &IndexWriter, traits: T) -> Result<(), Error>
        where
            T: Iterator<Item = &'a Trait>
    {
        traits.for_each(|t| {
            let schema = self.trait_schemas.get(&t.name).expect("Schema not found.");

            let mut trait_doc = Document::default();

            t.fields.iter().for_each(|(field, value)| {
                let schema_field = schema.get_field(&field.name).unwrap();

                match (&field.typ, value) {
                    (&FieldType::Text, FieldValue::Text(str)) => trait_doc.add_text(schema_field, &str),
                    _ => panic!("Type not supported yet")
                }

            })
        });

        Ok(())
    }

    fn build_schema(&mut self, t: &Trait) -> &Schema {
        // FIXME : to_string shouldn't be needed
        self.trait_schemas.entry(t.name.to_string()).or_insert_with(|| {
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
                        // FIXME : Allow different text indexing options
                        let mut options = TEXT;
                        if field.stored {
                            options = options.set_stored();
                        }
                        schema_builder.add_text_field(&field.name, options);
                    },
                    _ => panic!("Type not supported yet")
                }
            });

            schema_builder.build()
        })
    }
}

impl From<tantivy::TantivyError> for Error {
    fn from(err: tantivy::TantivyError) -> Self { Error::TantivyError(err) }
}

impl From<io::Error> for Error {
    fn from(_: io::Error) -> Self { Error::FileError }
}

pub enum Error {
    //    #[fail(display = "An error occurred: {}", _0)]
    TantivyError(tantivy::TantivyError),
    FileError,
    Other(String),
}
