use super::entity::{Entity, FieldValue, Record, Trait};
use crate::schema::schema::Record as SchemaRecord;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::iter::FromIterator;

pub fn serialize_entity(entity: Entity) {}

pub fn record_to_json_value<R: Record>(record: R) -> Value {
    let record_schema = record.record_schema();

    let default_fields = vec![(
        "_type".to_string(),
        Value::String(format!(
            "{}.{}",
            record.schema().name,
            record.record_schema().name()
        )),
    )];

    let values_iter = record.values().iter().flat_map(|(field_id, field_value)| {
        if let Some(field) = record_schema.field_by_id(*field_id) {
            Some((field.name.clone(), field_value_to_json(field_value)))
        } else {
            None
        }
    });

    Value::Object(Map::from_iter(
        default_fields.into_iter().chain(values_iter),
    ))
}

pub fn field_value_to_json(value: &FieldValue) -> Value {
    match value {
        FieldValue::String(str) => Value::String(str.to_string()),
        other => unimplemented!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::schema;
    use std::sync::Arc;

    #[test]
    fn serialize_trait() -> Result<(), failure::Error> {
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

        let trt = Trait::new(schema, "trait1").with_value_by_name("field1", "hey you");
        let json_value = serde_json::to_string(&record_to_json_value(trt))?;
        println!("{}", json_value);

        Ok(())
    }
}
