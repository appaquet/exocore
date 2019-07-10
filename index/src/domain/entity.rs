use super::schema::SchemaRecord;
use crate::domain::schema::{
    Schema, SchemaField, SchemaFieldId, SchemaStructId, SchemaTraitId, StructSchema, TraitSchema,
};
use crate::error::Error;
use std::collections::HashMap;
use std::sync::Arc;

pub type EntityId = String;
pub type EntityIdRef = str;
pub type TraitId = String;
pub type TraitIdRef = str;

#[derive(Serialize, Deserialize, Debug)]
pub struct Entity {
    pub id: EntityId,
    pub traits: Vec<Trait>,
}

impl Entity {
    pub fn new(id: EntityId) -> Entity {
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

    fn values(&self) -> &HashMap<SchemaFieldId, FieldValue>;

    fn values_mut(&mut self) -> &mut HashMap<SchemaFieldId, FieldValue>;

    fn value(&self, field: &SchemaField) -> Option<&FieldValue> {
        self.values().get(&field.id)
    }

    fn value_by_id(&self, id: SchemaFieldId) -> Option<&FieldValue> {
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
    schema_id: SchemaTraitId,
    values: HashMap<SchemaFieldId, FieldValue>,
}

impl Trait {
    pub fn new(schema: Arc<Schema>, trait_name: &str) -> Trait {
        let schema_id = schema
            .trait_by_name(trait_name)
            .expect("Trait doesn't exist in schema")
            .id;
        Trait {
            schema,
            schema_id,
            values: HashMap::new(),
        }
    }

    pub fn build(self) -> Result<Self, Error> {
        // TODO: maybe generate id
        Ok(self)
    }

    pub fn id(&self) -> &TraitId {
        match self.value_by_name(TraitSchema::TRAIT_ID_FIELD) {
            Some(FieldValue::String(id)) => id,
            other => panic!(
                "Trait didn't contain a trait_id or it wasn't a string: value={:?}",
                other
            ),
        }
    }

    pub fn with_id(self, id: TraitId) -> Self {
        self.with_value_by_name(TraitSchema::TRAIT_ID_FIELD, id)
    }
}

impl Record for Trait {
    type SchemaType = TraitSchema;

    fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    fn record_schema(&self) -> &Self::SchemaType {
        self.schema
            .trait_by_id(self.schema_id)
            .expect("Trait doesn't exist in schema")
    }

    fn values(&self) -> &HashMap<SchemaFieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<SchemaFieldId, FieldValue> {
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
        let record_schema = self.record_schema();
        let mut str_fmt = f.debug_struct(&record_schema.name);
        for (field_id, value) in &self.values {
            let field = record_schema
                .field_by_id(*field_id)
                .map(|f| f.name.to_string())
                .unwrap_or(format!("_{}", field_id));
            str_fmt.field(&field, value);
        }
        str_fmt.finish()
    }
}

///
///
///
pub struct Struct {
    schema: Arc<Schema>,
    id: SchemaStructId,
    values: HashMap<SchemaFieldId, FieldValue>,
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

    fn values(&self) -> &HashMap<SchemaFieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<SchemaFieldId, FieldValue> {
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
        let record_schema = self.record_schema();
        let mut str_fmt = f.debug_struct(&record_schema.name);
        for (field_id, value) in &self.values {
            let field = record_schema
                .field_by_id(*field_id)
                .map(|f| f.name.to_string())
                .unwrap_or(format!("_{}", field_id));
            str_fmt.field(&field, value);
        }
        str_fmt.finish()
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
