use crate::error::Error;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

pub type TraitId = u16;
pub type StructId = u16;
pub type FieldId = u16;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Schema {
    pub name: String,
    pub traits: Vec<Trait>,
    #[serde(skip)]
    pub traits_id: HashMap<TraitId, usize>,
    #[serde(skip)]
    pub traits_name: HashMap<String, usize>,
    #[serde(default = "Vec::new")]
    pub structs: Vec<Struct>,
    #[serde(skip)]
    pub structs_id: HashMap<StructId, usize>,
    #[serde(skip)]
    pub structs_name: HashMap<String, usize>,
}

impl Schema {
    pub fn parse(yaml: &str) -> Result<Schema, crate::error::Error> {
        let mut schema: Schema =
            serde_yaml::from_str(yaml).map_err(|err| Error::Schema(err.to_string()))?;

        for (stc_pos, stc) in schema.structs.iter_mut().enumerate() {
            if let Some(_other_struct) = schema.structs_id.insert(stc.id, stc_pos) {
                return Err(Error::Schema(format!(
                    "A struct with id {} already exists in schema",
                    stc.id
                )));
            }

            if let Some(_other_struct) = schema.structs_name.insert(stc.name.clone(), stc_pos) {
                return Err(Error::Schema(format!(
                    "A struct with name {} already exists in schema",
                    stc.name
                )));
            }

            for (field_pos, field) in stc.fields.iter_mut().enumerate() {
                if let Some(other_field) = stc.fields_id.insert(field.id, field_pos) {
                    return Err(Error::Schema(format!(
                        "Struct {} already had a field with id {}",
                        stc.name, field.id
                    )));
                }

                if let Some(other_field) = stc.fields_name.insert(field.name.clone(), field_pos) {
                    return Err(Error::Schema(format!(
                        "Struct {} already had a field with name {}",
                        stc.name, field.name
                    )));
                }
            }
        }

        for (trt_pos, trt) in schema.traits.iter_mut().enumerate() {
            if let Some(_other_trait) = schema.traits_id.insert(trt.id, trt_pos) {
                return Err(Error::Schema(format!(
                    "A trait with id {} already exists in schema",
                    trt.id
                )));
            }

            if let Some(_other_trait) = schema.traits_name.insert(trt.name.clone(), trt_pos) {
                return Err(Error::Schema(format!(
                    "A trait with name {} already exists in schema",
                    trt.name
                )));
            }

            for (field_pos, field) in trt.fields.iter_mut().enumerate() {
                if let Some(other_field) = trt.fields_id.insert(field.id, field_pos) {
                    return Err(Error::Schema(format!(
                        "Trait {} already had a field with id {}",
                        trt.name, field.id
                    )));
                }

                if let Some(other_field) = trt.fields_name.insert(field.name.clone(), field_pos) {
                    return Err(Error::Schema(format!(
                        "Trait {} already had a field with name {}",
                        trt.name, field.name
                    )));
                }
            }
        }

        // TODO: make sure referenced structs exist
        // TODO: make sure default values are parsable

        Ok(schema)
    }

    pub fn trait_by_id(&self, id: TraitId) -> Option<&Trait> {
        self.traits_id
            .get(&id)
            .and_then(|pos| self.traits.get(*pos))
    }

    pub fn trait_by_name(&self, name: &str) -> Option<&Trait> {
        self.traits_name
            .get(name)
            .and_then(|pos| self.traits.get(*pos))
    }

    pub fn struct_by_id(&self, id: StructId) -> Option<&Struct> {
        self.structs_id
            .get(&id)
            .and_then(|pos| self.structs.get(*pos))
    }

    pub fn struct_by_name(&self, name: &str) -> Option<&Struct> {
        self.structs_name
            .get(name)
            .and_then(|pos| self.structs.get(*pos))
    }
}

pub trait Record {
    fn name(&self) -> &str;

    fn field_by_id(&self, id: FieldId) -> Option<&Field>;
    fn field_by_name(&self, name: &str) -> Option<&Field>;
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Trait {
    pub id: u16,
    pub name: String,
    pub fields: Vec<Field>,
    #[serde(skip)]
    pub fields_id: HashMap<FieldId, usize>,
    #[serde(skip)]
    pub fields_name: HashMap<String, usize>,
}

impl Record for Trait {
    fn name(&self) -> &str {
        &self.name
    }

    fn field_by_id(&self, id: FieldId) -> Option<&Field> {
        self.fields_id
            .get(&id)
            .and_then(|pos| self.fields.get(*pos))
    }

    fn field_by_name(&self, name: &str) -> Option<&Field> {
        self.fields_name
            .get(name)
            .and_then(|pos| self.fields.get(*pos))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Struct {
    pub id: u16,
    pub name: String,
    pub fields: Vec<Field>,
    #[serde(skip)]
    pub fields_id: HashMap<FieldId, usize>,
    #[serde(skip)]
    pub fields_name: HashMap<String, usize>,
}

impl Record for Struct {
    fn name(&self) -> &str {
        &self.name
    }

    fn field_by_id(&self, id: FieldId) -> Option<&Field> {
        self.fields_id
            .get(&id)
            .and_then(|pos| self.fields.get(*pos))
    }

    fn field_by_name(&self, name: &str) -> Option<&Field> {
        self.fields_name
            .get(name)
            .and_then(|pos| self.fields.get(*pos))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Field {
    pub id: u16,
    pub name: String,
    #[serde(default = "default_true")]
    pub optional: bool,
    #[serde(default = "default_false")]
    pub indexed: bool,
    #[serde(rename = "type")]
    pub typ: FieldType,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Int,
    Bool,
    Struct(StructId),
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let schema_defaults = Schema::parse(
            r#"
        name: schema2
        traits:
            - id: 0
              name: trait2
              fields:
                - id: 0
                  name: my_field
                  type:
                      struct: 0
        "#,
        )
        .unwrap();
        assert_eq!("schema2", schema_defaults.name);

        println!("{}", serde_yaml::to_string(&schema_defaults).unwrap());
    }
}
