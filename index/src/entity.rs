use std::collections::HashMap;
use std::option::Option;
use std::vec::Vec;

trait Record {
    fn name(&self) -> String;
    fn fields(&self) -> Vec<Field>;
    fn get_field(&self, name: String) -> Option<Field>;
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum FieldType {
    Long,
    Text,
    Bool,
    Date,
}

#[derive(Debug)]
pub enum FieldValue {
    Long(u64),
    Text(String),
    Bool(bool),
    Date(u64), // TODO : Use chrono::DateTime
}

pub trait Indexable {
    fn id_fields(&self) -> Vec<&str>;
    fn sort_fields(&self) -> Option<&str>;
}

pub trait FieldAdder {
    fn with_field(
        &mut self,
        name: &str,
        typ: FieldType,
        value: FieldValue,
        indexed: bool,
        stored: bool,
    );
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: String,
    pub typ: FieldType,
    pub indexed: bool,
    pub stored: bool,
}

pub struct Entity {
    id: u64,
    creation_date: u64,
    modification_date: u64,
    traits: Vec<Trait>,
}

#[derive(Debug, PartialEq)]
pub enum TraitType {
    Unique,
    Repeated,
    SymmetricEdge,
    AsymmetricEdge,
}

// Concrete traits implementation
pub struct Trait {
    pub name: String,
    pub trait_type: TraitType,
    id_fields: Vec<String>,
    sort_field: Option<String>,
    pub fields: HashMap<Field, FieldValue>,
}

impl Trait {
    pub fn new(trait_type: TraitType, name: &str) -> Trait {
        let mut t = Trait {
            name: name.to_owned(),
            trait_type,
            id_fields: Vec::new(),
            sort_field: None,
            fields: HashMap::new(),
        };

        // Default fields
        t.fields.insert(
            Field {
                name: "creation_date".to_owned(),
                typ: FieldType::Long,
                indexed: true,
                stored: true,
            },
            FieldValue::Long(0),
        );
        t.fields.insert(
            Field {
                name: "modification_date".to_owned(),
                typ: FieldType::Long,
                indexed: true,
                stored: true,
            },
            FieldValue::Long(0),
        );

        t
    }
}

impl Indexable for Trait {
    fn id_fields(&self) -> Vec<&str> {
        self.id_fields.iter().map(AsRef::as_ref).collect()
    }

    fn sort_fields(&self) -> Option<&str> {
        self.sort_field.as_ref().map(AsRef::as_ref)
    }
}

impl FieldAdder for Trait {
    fn with_field(
        &mut self,
        name: &str,
        typ: FieldType,
        value: FieldValue,
        indexed: bool,
        stored: bool,
    ) {
        self.fields.insert(
            Field {
                name: name.to_owned(),
                typ,
                indexed,
                stored,
            },
            value,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_unique_trait() {
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

        assert_eq!(contact_trait.trait_type, TraitType::Unique);
        assert_eq!(contact_trait.name, "contact".to_owned());
        assert!(contact_trait.fields.iter().any(|(f, v)| f.name == "email"));
        assert!(contact_trait.fields.iter().any(|(f, v)| f.name == "name"));
    }
}
