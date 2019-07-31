use super::schema::RecordSchema;
use crate::domain::schema::{
    FieldSchema, Namespace, Schema, SchemaFieldId, StructSchema, TraitIdValue, TraitSchema,
};
use crate::error::Error;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

pub type EntityId = String;
pub type EntityIdRef = str;
pub type TraitId = String;
pub type TraitIdRef = str;

///
/// An entity is an object on which traits can be added to shape what it represents.
///   Ex: an email is an entity on which we added a trait "Email"
///       a note could be added to an email by adding a "Note" trait also
///
/// Traits can also represent relationship.
///   Ex: a "Child" trait can be used to add an entity into a collection
///
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
/// Trait that can be added to an entity, shaping its representation.
///
pub struct Trait {
    schema: Arc<Schema>,
    namespace: Arc<Namespace>,
    trait_schema: Arc<TraitSchema>,
    trait_id: TraitId,
    values: HashMap<SchemaFieldId, FieldValue>,
}

impl Trait {
    // TODO: Should be faillible
    pub fn new(schema: Arc<Schema>, full_name: &str) -> Trait {
        Self::new_try(schema, full_name).expect("Error creating trait")
    }

    pub fn new_try(schema: Arc<Schema>, full_name: &str) -> Result<Trait, Error> {
        let (ns_name, trait_name) =
            super::schema::parse_record_full_name(full_name).ok_or_else(|| {
                Error::Schema(format!("Couldn't parse record full name '{}'", full_name))
            })?;

        let namespace = schema
            .namespace_by_name(ns_name)
            .ok_or_else(|| {
                Error::Schema(format!("Couldn't find namespace with name '{}'", ns_name))
            })?
            .clone();

        let trait_schema = namespace
            .trait_by_name(trait_name)
            .ok_or_else(|| {
                Error::Schema(format!(
                    "Couldn't find trait with name '{}' in namespace '{}'",
                    trait_name, ns_name
                ))
            })?
            .clone();

        let mut trt = Trait {
            schema,
            namespace,
            trait_schema,
            trait_id: "".to_owned(),
            values: HashMap::new(),
        };

        // TODO: Prevent this
        let trait_id = trt.generate_id()?;
        trt.trait_id = trait_id;

        Ok(trt)
    }

    fn generate_id(&self) -> Result<String, Error> {
        let current_id_value = self
            .value_by_name(TraitSchema::TRAIT_ID_FIELD)
            .and_then(|fv| match fv {
                FieldValue::String(s) => Some(s.clone()),
                _ => None,
            });

        match self.trait_schema.id_field() {
            TraitIdValue::Specified => {
                current_id_value
                    .ok_or_else(|| {
                        Error::DataIntegrity(format!(
                            "Trait with schema_trait_id={} didn't have a value for id field but should have been specified",
                            self.trait_schema.id(),
                        ))
                    })
            }
            TraitIdValue::Generated => {
                match current_id_value {
                    Some(id) => Ok(id),
                    None =>
                        Ok(Uuid::new_v4().to_string()),
                }
            }
            TraitIdValue::Static(id) => {
                Ok(id.clone())
            }
            TraitIdValue::Field(_id) => {
                // TODO: To implement
                unimplemented!()
            }
            TraitIdValue::Fields(_ids) => {
                // TODO: To implement
                unimplemented!()
            }
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        // TODO:
        Ok(())
    }

    pub fn id(&self) -> &TraitId {
        &self.trait_id
    }

    pub fn with_id<V: Into<TraitId>>(mut self, id: V) -> Self {
        let trait_id = id.into();
        self.trait_id = trait_id.clone();
        self.with_value_by_name(TraitSchema::TRAIT_ID_FIELD, trait_id.clone())
    }
}

impl Record for Trait {
    type SchemaType = TraitSchema;

    fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    fn namespace(&self) -> &Arc<Namespace> {
        &self.namespace
    }

    fn record_schema(&self) -> &Arc<Self::SchemaType> {
        &self.trait_schema
    }

    fn values(&self) -> &HashMap<SchemaFieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<SchemaFieldId, FieldValue> {
        &mut self.values
    }
}

impl std::fmt::Debug for Trait {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        self.debug_fmt(f)
    }
}

impl PartialEq for Trait {
    fn eq(&self, other: &Self) -> bool {
        if self.namespace.name() != other.namespace.name() {
            return false;
        }

        if self.trait_schema.id() != other.trait_schema.id() {
            return false;
        }

        self.values == other.values
    }
}

///
/// Common (Rust) trait between `Struct` and `Trait`, since both are a collection of fields.
///
pub trait Record: Sized {
    type SchemaType: RecordSchema;

    fn schema(&self) -> &Arc<Schema>;

    fn namespace(&self) -> &Arc<Namespace>;

    fn record_schema(&self) -> &Arc<Self::SchemaType>;

    fn full_name(&self) -> String {
        format!(
            "{}.{}",
            self.namespace().name(),
            self.record_schema().name()
        )
    }

    fn values(&self) -> &HashMap<SchemaFieldId, FieldValue>;

    fn values_mut(&mut self) -> &mut HashMap<SchemaFieldId, FieldValue>;

    fn value(&self, field: &FieldSchema) -> Option<&FieldValue> {
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

    fn debug_fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let record_schema = self.record_schema();
        let name = self.full_name();
        let mut str_fmt = f.debug_struct(&name);
        for (field_id, value) in self.values() {
            let field = record_schema
                .field_by_id(*field_id)
                .map(|f| f.name.to_string())
                .unwrap_or_else(|| format!("_{}", field_id));
            str_fmt.field(&field, value);
        }
        str_fmt.finish()
    }
}

///
/// Structure with field-value pairs that can be used as a value of any field in a `Record`
///
pub struct Struct {
    schema: Arc<Schema>,
    namespace: Arc<Namespace>,
    struct_schema: Arc<StructSchema>,
    values: HashMap<SchemaFieldId, FieldValue>,
}

impl Struct {
    // TODO: Should be faillible
    pub fn new(schema: Arc<Schema>, full_name: &str) -> Struct {
        let (ns_name, struct_name) = super::schema::parse_record_full_name(full_name)
            .expect("Couldn't parse struct full name");

        let namespace = schema
            .namespace_by_name(ns_name)
            .expect("Namespace doesn't exist in schema")
            .clone();

        let struct_schema = namespace
            .struct_by_name(struct_name)
            .expect("Struct doesn't exist in namespace")
            .clone();

        Struct {
            schema,
            namespace,
            struct_schema,
            values: HashMap::new(),
        }
    }
}

impl Record for Struct {
    type SchemaType = StructSchema;

    fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    fn namespace(&self) -> &Arc<Namespace> {
        &self.namespace
    }

    fn record_schema(&self) -> &Arc<Self::SchemaType> {
        &self.struct_schema
    }

    fn values(&self) -> &HashMap<SchemaFieldId, FieldValue> {
        &self.values
    }

    fn values_mut(&mut self) -> &mut HashMap<SchemaFieldId, FieldValue> {
        &mut self.values
    }
}

impl std::fmt::Debug for Struct {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        self.debug_fmt(f)
    }
}

impl PartialEq for Struct {
    fn eq(&self, other: &Self) -> bool {
        if self.namespace.name() != other.namespace.name() {
            return false;
        }

        if self.struct_schema.id() != other.struct_schema.id() {
            return false;
        }

        self.values == other.values
    }
}

///
/// Value of a field of a record
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
    fn trait_parse_basic() -> Result<(), failure::Error> {
        let schema = Arc::new(Schema::parse(
            r#"
        namespaces:
            - name: exocore
              traits:
                - id: 0
                  name: trait1
                  fields:
                    - id: 0
                      name: field1
                      type: string
              structs:
                - id: 0
                  name: struct1
                  fields:
                    - id: 0
                      name: field1
                      type: string
        "#,
        )?);

        let trt =
            Trait::new(schema.clone(), "exocore.trait1").with_value_by_name("field1", "hello");
        assert_eq!(
            FieldValue::String("hello".to_string()),
            *trt.value_by_name("field1").unwrap()
        );
        assert_eq!(
            FieldValue::String("hello".to_string()),
            *trt.value_by_id(0).unwrap()
        );
        assert_eq!(None, trt.value_by_name("doesnt_exist"));
        debug!("Trait: {:?}", trt);

        let stc = Struct::new(schema, "exocore.struct1").with_value_by_name("field1", "hello");
        assert_eq!(
            FieldValue::String("hello".to_string()),
            *stc.value_by_name("field1").unwrap()
        );
        assert_eq!(
            FieldValue::String("hello".to_string()),
            *stc.value_by_id(0).unwrap()
        );
        assert_eq!(None, stc.value_by_name("doesnt_exist"));
        debug!("Struct: {:?}", stc);

        Ok(())
    }
}
