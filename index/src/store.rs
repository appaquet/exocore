use exocore_data;

use tantivy::Index;
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
        contact_trait.with_field("email", FieldType::Text, true, true);
        contact_trait.with_field("name", FieldType::Text, true, true);
        /* END BOOTSTRAP */

        let index = Index::create_from_tempdir(self.build_schema(&contact_trait).clone())?;
        let mut index_writer = index.writer(50_000_000)?;
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

            t.fields.iter().for_each(|field: &Field| {
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
                    }
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
