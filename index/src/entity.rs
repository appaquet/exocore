use std::collections::HashSet;
use std::option::Option;
use std::vec::Vec;

trait Record {
    fn name(&self) -> String;
    fn fields(&self) -> Vec<Field>;
    fn get_field(&self, name: String) -> Option<Field>;
}

#[derive(Debug, PartialEq)]
pub enum TraitType {
    Unique,
    Repeated,
    SymmetricEdge,
    AsymmetricEdge,
}

trait Indexable {
    fn id_fields(&self) -> Vec<&str>;
    fn sort_fields(&self) -> Option<&str>;
}

trait FieldAdder {
    fn with_field(&mut self, field: &str);
}

pub struct Field {
    name: String,
    typ: String,
    indexed: bool,
    searchable: bool
}

pub struct Entity {
    id: u64,
    creation_date: u64,
    modification_date: u64,
    traits: Vec<Trait>
}

static DEFAULT_FIELDS: [&'static str; 2] = ["creation_date", "modification_date"];

// Concrete traits implementation
pub struct Trait {
    pub name: String,
    pub trait_type: TraitType,
    id_fields: Vec<String>,
    sort_field: Option<String>,
    pub fields: HashSet<String> // TODO : Change to Field type
}

impl Trait {
    pub fn new(trait_type: TraitType, name: &str) -> Trait {
        let mut t = Trait {
            name: name.to_owned(),
            trait_type,
            id_fields: Vec::new(),
            sort_field: None,
            fields: HashSet::new()
        };

        for f in DEFAULT_FIELDS.iter() {
            t.fields.insert(f.to_string());
        };

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
    fn with_field(&mut self, field: &str) {
        self.fields.insert(field.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_unique_trait() {
        let mut contact_trait = Trait::new(TraitType::Unique, "contact");
        contact_trait.with_field("email");
        contact_trait.with_field("name");

        assert_eq!(contact_trait.trait_type, TraitType::Unique);
        assert_eq!(contact_trait.name, "contact".to_owned());
        assert!(contact_trait.fields.contains("email"));
        assert!(contact_trait.fields.contains("name"));
    }
}