use crate::error::Error;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub type SchemaRecordId = u16;
pub type SchemaTraitId = SchemaRecordId;
pub type SchemaStructId = SchemaRecordId;
pub type SchemaFieldId = u16;

///
/// Schema of a Cell that contains a collection of namespace
///
pub struct Schema {
    namespaces: Vec<Arc<Namespace>>,
    namespaces_name: HashMap<String, Arc<Namespace>>,
}

impl Schema {
    pub fn parse(yaml: &str) -> Result<Schema, Error> {
        SerializableSchema::parse(yaml)?.validate()
    }

    pub fn add_namespace(&mut self, namespace: Namespace) -> Result<(), Error> {
        let namespace = Arc::new(namespace);

        let name = namespace.name.clone();
        if self.namespaces_name.contains_key(&name) {
            return Err(Error::Schema(format!(
                "A namespace with this name already exists in the schema: {}",
                name
            )));
        }

        self.namespaces.push(namespace.clone());
        self.namespaces_name.insert(name, namespace.clone());

        Ok(())
    }

    pub fn namespace_by_name(&self, name: &str) -> Option<&Arc<Namespace>> {
        self.namespaces_name.get(name)
    }

    pub fn trait_by_full_name(&self, full_name: &str) -> Option<&Arc<TraitSchema>> {
        let (ns_name, trait_name) = parse_record_full_name(full_name)?;
        self.namespace_by_name(ns_name)
            .and_then(|ns| ns.trait_by_name(trait_name))
    }

    pub fn struct_by_full_name(&self, full_name: &str) -> Option<&Arc<StructSchema>> {
        let (ns_name, trait_name) = parse_record_full_name(full_name)?;
        self.namespace_by_name(ns_name)
            .and_then(|ns| ns.struct_by_name(trait_name))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SerializableSchema {
    namespaces: Vec<SerializableNamespace>,
}

impl SerializableSchema {
    pub fn parse(yaml: &str) -> Result<SerializableSchema, Error> {
        let deser_schema =
            serde_yaml::from_str(yaml).map_err(|err| Error::Schema(err.to_string()))?;
        Ok(deser_schema)
    }

    pub fn validate(self) -> Result<Schema, Error> {
        let mut namespaces = Vec::new();
        let mut namespaces_name = HashMap::new();
        for namespace in self.namespaces.into_iter() {
            let namespace = Arc::new(namespace.validate()?);

            if namespaces_name.contains_key(&namespace.name) {
                return Err(Error::Schema(format!(
                    "A namespace with this name already exists in the schema: {}",
                    namespace.name,
                )));
            }

            namespaces.push(namespace.clone());
            namespaces_name.insert(namespace.name.clone(), namespace.clone());
        }

        Ok(Schema {
            namespaces,
            namespaces_name,
        })
    }
}

///
/// A namespace contains traits and structs definition for an application installed / used
/// in the Cell.
///
pub struct Namespace {
    name: String,
    traits_id: HashMap<SchemaTraitId, Arc<TraitSchema>>,
    traits_name: HashMap<String, Arc<TraitSchema>>,
    structs_id: HashMap<SchemaStructId, Arc<StructSchema>>,
    structs_name: HashMap<String, Arc<StructSchema>>,
}

impl Namespace {
    pub fn parse(yaml: &str) -> Result<Namespace, Error> {
        SerializableNamespace::parse(yaml)?.validate()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn trait_by_id(&self, id: SchemaTraitId) -> Option<&Arc<TraitSchema>> {
        self.traits_id.get(&id)
    }

    pub fn trait_by_name(&self, name: &str) -> Option<&Arc<TraitSchema>> {
        self.traits_name.get(name)
    }

    pub fn struct_by_id(&self, id: SchemaStructId) -> Option<&Arc<StructSchema>> {
        self.structs_id.get(&id)
    }

    pub fn struct_by_name(&self, name: &str) -> Option<&Arc<StructSchema>> {
        self.structs_name.get(name)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SerializableNamespace {
    name: String,
    traits: Vec<TraitSchema>,
    #[serde(default = "Vec::new")]
    structs: Vec<StructSchema>,
}

impl SerializableNamespace {
    pub fn parse(yaml: &str) -> Result<SerializableNamespace, Error> {
        let namespace = serde_yaml::from_str(yaml).map_err(|err| Error::Schema(err.to_string()))?;
        Ok(namespace)
    }

    pub fn validate(self) -> Result<Namespace, Error> {
        if !is_valid_name(&self.name) {
            return Err(Error::Schema(format!(
                "Namespace name can only be alphanumeric with underscore, but can only start with letters: name={}",
                self.name
            )));
        }

        let mut structs_id = HashMap::<SchemaStructId, Arc<StructSchema>>::new();
        let mut structs_name = HashMap::<String, Arc<StructSchema>>::new();
        for mut stc in self.structs.into_iter() {
            if !is_valid_name(&stc.name) {
                return Err(Error::Schema(format!(
                    "Struct name can only be alphanumeric with underscore, but can only start with letters: name={}",
                    stc.name
                )));
            }

            for (field_pos, field) in stc.fields.iter_mut().enumerate() {
                if !is_valid_name(&field.name) && !field.is_special() {
                    return Err(Error::Schema(format!(
                        "Field name can only be alphanumeric with underscore, but can only start with letters: name={}",
                        field.name
                    )));
                }

                if let Some(_other_field) = stc.fields_id.insert(field.id, field_pos) {
                    return Err(Error::Schema(format!(
                        "Struct {} already had a field with id {}",
                        stc.name, field.id
                    )));
                }

                if let Some(_other_field) = stc.fields_name.insert(field.name.clone(), field_pos) {
                    return Err(Error::Schema(format!(
                        "Struct {} already had a field with name {}",
                        stc.name, field.name
                    )));
                }
            }

            let stc_arc = Arc::new(stc);
            if let Some(_other_struct) = structs_id.insert(stc_arc.id, stc_arc.clone()) {
                return Err(Error::Schema(format!(
                    "A struct with id {} already exists in namespace",
                    stc_arc.id
                )));
            }

            if let Some(_other_struct) = structs_name.insert(stc_arc.name.clone(), stc_arc.clone())
            {
                return Err(Error::Schema(format!(
                    "A struct with name {} already exists in namespace",
                    stc_arc.name
                )));
            }
        }

        let mut traits_id = HashMap::<SchemaTraitId, Arc<TraitSchema>>::new();
        let mut traits_name = HashMap::<String, Arc<TraitSchema>>::new();
        for mut trt in self.traits.into_iter() {
            if !is_valid_name(&trt.name) {
                return Err(Error::Schema(format!(
                    "Trait name can only be alphanumeric with underscore, but can only start with letters: name={}",
                    trt.name
                )));
            }

            let default_fields = TraitSchema::default_fields();
            for field in default_fields {
                trt.fields.push(field);
            }

            for (field_pos, field) in trt.fields.iter_mut().enumerate() {
                if !is_valid_name(&field.name) && !field.is_special() {
                    return Err(Error::Schema(format!(
                        "Field name can only be alphanumeric with underscore, but can only start with letters: name={}",
                        field.name
                    )));
                }

                if let Some(_other_field) = trt.fields_id.insert(field.id, field_pos) {
                    return Err(Error::Schema(format!(
                        "Trait {} already had a field with id {}",
                        trt.name, field.id
                    )));
                }

                if let Some(_other_field) = trt.fields_name.insert(field.name.clone(), field_pos) {
                    return Err(Error::Schema(format!(
                        "Trait {} already had a field with name {}",
                        trt.name, field.name
                    )));
                }
            }

            let trt_arc = Arc::new(trt);
            if let Some(_other_trait) = traits_id.insert(trt_arc.id, trt_arc.clone()) {
                return Err(Error::Schema(format!(
                    "A trait with id {} already exists in namespace",
                    trt_arc.id
                )));
            }

            if let Some(_other_trait) = traits_name.insert(trt_arc.name.clone(), trt_arc.clone()) {
                return Err(Error::Schema(format!(
                    "A trait with name {} already exists in namespace",
                    trt_arc.name
                )));
            }
        }

        Ok(Namespace {
            name: self.name,
            traits_id,
            traits_name,
            structs_id,
            structs_name,
        })
    }
}

///
/// Common trait of Trait and Structs. Both have a name and fields.
///
pub trait SchemaRecord {
    fn id(&self) -> SchemaRecordId;
    fn name(&self) -> &str;

    fn fields(&self) -> &[SchemaField];
    fn field_by_id(&self, id: SchemaFieldId) -> Option<&SchemaField>;
    fn field_by_name(&self, name: &str) -> Option<&SchemaField>;
}

///
/// Schema definition of a Trait. A Trait can be added to an Entity to define its
/// behaviour.
///
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TraitSchema {
    id: SchemaTraitId,
    name: String,
    fields: Vec<SchemaField>,
    #[serde(skip)]
    fields_id: HashMap<SchemaFieldId, usize>,
    #[serde(skip)]
    fields_name: HashMap<String, usize>,
}

impl TraitSchema {
    pub const TRAIT_ID_FIELD: &'static str = "_id";
    pub const CREATION_DATE_FIELD: &'static str = "creation_date";
    pub const MODIFICATION_DATE_FIELD: &'static str = "modification_date";

    fn default_fields() -> Vec<SchemaField> {
        vec![
            SchemaField {
                id: 65400,
                name: Self::TRAIT_ID_FIELD.to_owned(),
                typ: FieldType::String,
                indexed: false, // special case, it's indexed & stored in another way
                optional: false,
            },
            SchemaField {
                id: 65401,
                name: Self::CREATION_DATE_FIELD.to_owned(),
                typ: FieldType::Int, // TODO: date
                indexed: true,
                optional: false,
            },
            SchemaField {
                id: 65402,
                name: Self::MODIFICATION_DATE_FIELD.to_owned(),
                typ: FieldType::Int, // TODO: date
                indexed: true,
                optional: false,
            },
        ]
    }
}

impl SchemaRecord for TraitSchema {
    fn id(&self) -> SchemaTraitId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn fields(&self) -> &[SchemaField] {
        &self.fields
    }

    fn field_by_id(&self, id: SchemaFieldId) -> Option<&SchemaField> {
        self.fields_id
            .get(&id)
            .and_then(|pos| self.fields.get(*pos))
    }

    fn field_by_name(&self, name: &str) -> Option<&SchemaField> {
        self.fields_name
            .get(name)
            .and_then(|pos| self.fields.get(*pos))
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum IdField {
    Generated,
    Field(SchemaFieldId),
    Fields(Vec<SchemaFieldId>),
}

///
/// Schema definition of a Struct. A struct can be used in a field of a Trait or Struct.
///
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StructSchema {
    id: u16,
    name: String,
    fields: Vec<SchemaField>,
    #[serde(skip)]
    fields_id: HashMap<SchemaFieldId, usize>,
    #[serde(skip)]
    fields_name: HashMap<String, usize>,
}

impl SchemaRecord for StructSchema {
    fn id(&self) -> SchemaStructId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn fields(&self) -> &[SchemaField] {
        &self.fields
    }

    fn field_by_id(&self, id: SchemaFieldId) -> Option<&SchemaField> {
        self.fields_id
            .get(&id)
            .and_then(|pos| self.fields.get(*pos))
    }

    fn field_by_name(&self, name: &str) -> Option<&SchemaField> {
        self.fields_name
            .get(name)
            .and_then(|pos| self.fields.get(*pos))
    }
}

///
/// Field of a Trait of Struct.
///
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SchemaField {
    pub id: u16,
    pub name: String,
    #[serde(default = "default_true")]
    pub optional: bool,
    #[serde(default = "default_false")]
    pub indexed: bool,
    #[serde(rename = "type")]
    pub typ: FieldType,
}

impl SchemaField {
    fn is_special(&self) -> bool {
        self.id >= 65400
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Int,
    Bool,
    Struct(SchemaStructId),
    // TODO: Date time
    // TODO: Consistent timestamp
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

fn is_valid_name(name: &str) -> bool {
    if name.len() < 2 {
        return false;
    }

    let last_pos = name.len() - 1;
    name.chars().enumerate().all(|(pos, chr)| match chr {
        'a'..='z' | 'A'..='Z' => true,
        '0'..='9' if pos != 0 => true,
        '_' if pos != 0 && pos != last_pos => true,
        _ => false,
    })
}

pub(crate) fn parse_record_full_name(full_name: &str) -> Option<(&str, &str)> {
    let splits = full_name.split('.').collect::<Vec<_>>();
    if splits.len() == 2 {
        Some((splits[0], splits[1]))
    } else {
        None
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn names_validation() {
        assert!(!is_valid_name("a"));
        assert!(is_valid_name("hello"));
        assert!(is_valid_name("h3ll0"));
        assert!(is_valid_name("he"));
        assert!(!is_valid_name("_hello"));
        assert!(!is_valid_name("0hello"));
        assert!(!is_valid_name("hello_"));
        assert!(!is_valid_name("__"));
        assert!(!is_valid_name("hel.lo"));
    }

    #[test]
    fn schema_records_by_full_name() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        assert!(schema.trait_by_full_name("exocore.contact").is_some());
        assert!(schema.trait_by_full_name("bla.contact").is_none());
        assert!(schema.trait_by_full_name("exocore.something").is_none());
        Ok(())
    }

    #[test]
    fn traits_default_fields() {
        let ns = Namespace::parse(
            r#"
        name: ns1
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
        assert_eq!("ns1", ns.name);

        let trt = ns.trait_by_name("trait2").unwrap();
        assert!(trt.field_by_name(TraitSchema::TRAIT_ID_FIELD).is_some());
        assert!(trt
            .field_by_name(TraitSchema::CREATION_DATE_FIELD)
            .is_some());
        assert!(trt
            .field_by_name(TraitSchema::MODIFICATION_DATE_FIELD)
            .is_some());

        //  TODO: Put back  assert!(serde_yaml::to_string(&ns).is_ok());
    }

    #[test]
    fn forbid_duplicate_traits() {
        let ns_result = Namespace::parse(
            r#"
        name: ns2
        traits:
            - id: 0
              name: trait1
            - id: 0
              name: trait2
        "#,
        );

        // 2 traits with same id
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns2 
        traits:
            - id: 0
              name: trait1
            - id: 1
              name: trait1
        "#,
        );

        // 2 traits with same name
        assert!(ns_result.is_err());
    }

    #[test]
    fn forbid_duplicate_structs() {
        let ns_result = Namespace::parse(
            r#"
        name: ns2
        traits:
            - id: 0
              name: struct1
            - id: 0
              name: struct2
        "#,
        );

        // 2 structs with same id
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns2
        traits:
            - id: 0
              name: struct1
            - id: 1
              name: struct1
        "#,
        );

        // 2 structs with same name
        assert!(ns_result.is_err());
    }

    #[test]
    fn forbid_duplicate_fields() {
        let ns_result = Namespace::parse(
            r#"
        name: ns2
        traits:
            - id: 0
              name: struct1
                - id: 0
                  name: field1
                - id: 0
                  name: field2
        "#,
        );

        // 2 fields with same id
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns2
        traits:
            - id: 0
              name: struct1
                - id: 0
                  name: field1
                - id: 1
                  name: field1
        "#,
        );

        // 2 fields with same name
        assert!(ns_result.is_err());
    }

    #[test]
    fn forbid_invalid_names() {
        let ns_result = Namespace::parse(
            r#"
        name: 0schema
        traits:
            - id: 0
              name: field1
        "#,
        );
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns
        traits:
            - id: 0
              name: 0trait
        "#,
        );
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns
        structs:
            - id: 0
              name: 0struct
        "#,
        );
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns
        traits:
            - id: 0
              name: trt
              fields:
                - id: 0
                  name: 0field
                  type: string
        "#,
        );
        assert!(ns_result.is_err());

        let ns_result = Namespace::parse(
            r#"
        name: ns
        structs:
            - id: 0
              name: struct
              fields:
                - id: 0
                  name: 0field
                  type: string
        "#,
        );
        assert!(ns_result.is_err());
    }

    #[test]
    fn generated_id_field() {
        // TODO:
    }

    pub fn create_test_schema() -> Arc<Schema> {
        Arc::new(
            Schema::parse(
                r#"
        namespaces:
          - name: exocore
            traits:
              - id: 0
                name: contact
                fields:
                  - id: 0
                    name: name
                    type: string
                    indexed: true
                  - id: 1
                    name: email
                    type: string
                    indexed: true
              - id: 1
                name: email
                fields:
                  - id: 0
                    name: subject
                    type: string
                    indexed: true
                  - id: 1
                    name: body
                    type: string
                    indexed: true
        "#,
            )
            .unwrap(),
        )
    }
}
