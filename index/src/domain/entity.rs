use super::schema::SchemaRecord;
use crate::domain::schema::{
    FieldId, Schema, SchemaField, StructId, StructSchema, TraitId, TraitSchema,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub traits: Vec<Trait>,
}

impl Entity {
    pub fn new(id: String) -> Entity {
        Entity {
            id,
            traits: Vec::new(),
        }
    }

    pub fn with_trait(mut self, trt: Trait) -> Self {
        self.traits.push(trt);
        self
    }
}

///
///
///
pub trait Record: Sized {
    type SchemaType: SchemaRecord;

    fn schema(&self) -> &Arc<Schema>;

    fn record_schema(&self) -> &Self::SchemaType;

    fn values(&self) -> &HashMap<FieldId, FieldValue>;

    fn values_mut(&mut self) -> &mut HashMap<FieldId, FieldValue>;

    fn value(&self, field: &SchemaField) -> Option<&FieldValue> {
        self.values().get(&field.id)
    }

    fn value_by_id(&self, id: FieldId) -> Option<&FieldValue> {
        self.values().get(&id)
    }

    fn value_by_name(&self, field_name: &str) -> Option<&FieldValue> {
        let field = self.record_schema().field_by_name(field_name)?;
        self.values().get(&field.id)
    }

    fn with_value_by_name<V: Into<FieldValue>>(mut self, field_name: &str, value: V) -> Self {
        if let Some(field_id) = self.record_schema().field_by_name(field_name).map(|f| f.id) {
            self.values_mut().insert(field_id, value.into());
        }
        self
    }
}

///
///
///
pub struct Trait {
    schema: Arc<Schema>,
    id: TraitId,
    values: HashMap<FieldId, FieldValue>,
}

impl Trait {
    pub fn new(schema: Arc<Schema>, trait_name: &str) -> Trait {
        let trait_id = schema
            .trait_by_name(trait_name)
            .expect("Trait doesn't exist in schema")
            .id;
        Trait {
            schema,
            id: trait_id,
            values: HashMap::new(),
        }
    }
}

impl Record for Trait {
    type SchemaType = TraitSchema;

    fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    fn record_schema(&self) -> &Self::SchemaType {
        self.schema
            .trait_by_id(self.id)
            .expect("Trait doesn't exist in schema")
    }

    fn values(&self) -> &HashMap<FieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<FieldId, FieldValue> {
        &mut self.values
    }
}

impl PartialEq for Trait {
    fn eq(&self, other: &Self) -> bool {
        unimplemented!()
    }
}

impl std::fmt::Debug for Trait {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        unimplemented!()
    }
}

///
///
///
pub struct Struct {
    schema: Arc<Schema>,
    id: StructId,
    values: HashMap<FieldId, FieldValue>,
}

impl Struct {
    pub fn new(schema: Arc<Schema>, struct_name: &str) -> Struct {
        let struct_id = schema
            .struct_by_name(struct_name)
            .expect("Struct doesn't exist in schema")
            .id;
        Struct {
            schema,
            id: struct_id,
            values: HashMap::new(),
        }
    }
}

impl Record for Struct {
    type SchemaType = StructSchema;
    fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    fn record_schema(&self) -> &Self::SchemaType {
        self.schema
            .struct_by_id(self.id)
            .expect("Struct doesn't exist in schema")
    }

    fn values(&self) -> &HashMap<FieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<FieldId, FieldValue> {
        &mut self.values
    }
}

impl PartialEq for Struct {
    fn eq(&self, other: &Self) -> bool {
        unimplemented!()
    }
}

impl std::fmt::Debug for Struct {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        unimplemented!()
    }
}

///
///
///
#[derive(PartialEq, Debug)]
pub enum FieldValue {
    String(String),
    Int(i64),
    Struct(Struct),
    Map(HashMap<String, FieldValue>),
}

impl From<&str> for FieldValue {
    fn from(v: &str) -> FieldValue {
        FieldValue::String(v.to_string())
    }
}

impl From<String> for FieldValue {
    fn from(v: String) -> FieldValue {
        FieldValue::String(v)
    }
}

impl From<Struct> for FieldValue {
    fn from(v: Struct) -> FieldValue {
        FieldValue::Struct(v)
    }
}

impl From<i64> for FieldValue {
    fn from(v: i64) -> FieldValue {
        FieldValue::Int(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() -> Result<(), failure::Error> {
        let schema = Arc::new(Schema::parse(
            r#"
        name: schema1
        traits:
            - id: 0
              name: trait1
              fields:
                - id: 0
                  name: field1
                  type: string
        "#,
        )?);

        let trt = Trait::new(schema, "trait1").with_value_by_name("field1", "hello");

        assert_eq!(
            FieldValue::String("hello".to_string()),
            *trt.value_by_name("field1").unwrap()
        );
        assert_eq!(
            FieldValue::String("hello".to_string()),
            *trt.value_by_id(0).unwrap()
        );
        assert_eq!(None, trt.value_by_name("doesnt_exist"));

        Ok(())
    }
}
