use super::schema;
use super::schema::Record as SchemaRecord;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Entity {
    pub id: String,
    pub traits: Vec<Trait>,
}

pub trait Record: Sized {
    type SchemaType: schema::Record;

    fn schema(&self) -> &Arc<schema::Schema>;

    fn record_schema(&self) -> &Self::SchemaType;

    fn values(&self) -> &HashMap<schema::FieldId, FieldValue>;

    fn values_mut(&mut self) -> &mut HashMap<schema::FieldId, FieldValue>;

    fn value_by_id(&self, id: schema::FieldId) -> Option<&FieldValue> {
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

pub struct Trait {
    schema: Arc<schema::Schema>,
    id: schema::TraitId,
    values: HashMap<schema::FieldId, FieldValue>,
}

impl Trait {
    pub fn new(schema: Arc<schema::Schema>, trait_name: &str) -> Trait {
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
    type SchemaType = schema::Trait;

    fn schema(&self) -> &Arc<schema::Schema> {
        &self.schema
    }

    fn record_schema(&self) -> &Self::SchemaType {
        self.schema
            .trait_by_id(self.id)
            .expect("Trait doesn't exist in schema")
    }

    fn values(&self) -> &HashMap<schema::FieldId, FieldValue> {
        &self.values
    }
    fn values_mut(&mut self) -> &mut HashMap<schema::FieldId, FieldValue> {
        &mut self.values
    }
}

pub struct Struct {
    schema: Arc<schema::Schema>,
    id: schema::StructId,
    values: HashMap<schema::FieldId, FieldValue>,
}

impl Record for Struct {
    type SchemaType = schema::Struct;
    fn schema(&self) -> &Arc<schema::Schema> {
        &self.schema
    }

    fn record_schema(&self) -> &Self::SchemaType {
        self.schema
            .struct_by_id(self.id)
            .expect("Struct doesn't exist in schema")
    }

    fn values(&self) -> &HashMap<schema::FieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<schema::FieldId, FieldValue> {
        &mut self.values
    }
}

#[derive(PartialEq, Debug)]
pub enum FieldValue {
    String(String),
    Int(i64),
}

impl From<&str> for FieldValue {
    fn from(string: &str) -> FieldValue {
        FieldValue::String(string.to_string())
    }
}

impl From<String> for FieldValue {
    fn from(string: String) -> FieldValue {
        FieldValue::String(string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() -> Result<(), failure::Error> {
        let schema = Arc::new(schema::Schema::parse(
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
