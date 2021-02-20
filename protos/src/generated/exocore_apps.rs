#[derive(Serialize, Deserialize, Clone, PartialEq, ::prost::Message)]
pub struct Manifest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub version: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub public_key: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    #[serde(default)]
    pub path: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "4")]
    #[serde(default)]
    pub schemas: ::prost::alloc::vec::Vec<ManifestSchema>,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, ::prost::Message)]
pub struct ManifestSchema {
    #[prost(oneof = "manifest_schema::Source", tags = "1, 2")]
    #[serde(flatten)]
    pub source: ::core::option::Option<manifest_schema::Source>,
}
/// Nested message and enum types in `ManifestSchema`.
pub mod manifest_schema {
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Source {
        #[prost(string, tag = "1")]
        File(::prost::alloc::string::String),
        #[prost(bytes, tag = "2")]
        #[serde(
            serialize_with = "crate::base64::as_base64",
            deserialize_with = "crate::base64::from_base64"
        )]
        Bytes(::prost::alloc::vec::Vec<u8>),
    }
}
/// Message sent to application running in WASM from runtime.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InMessage {
    #[prost(enumeration = "InMessageType", tag = "1")]
    pub r#type: i32,
    #[prost(bytes = "vec", tag = "2")]
    pub data: ::prost::alloc::vec::Vec<u8>,
}
/// Message sent from application running in WASM to runtime.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OutMessage {
    #[prost(enumeration = "OutMessageType", tag = "1")]
    pub r#type: i32,
    #[prost(bytes = "vec", tag = "2")]
    pub data: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum InMessageType {
    InMsgInvalid = 0,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum OutMessageType {
    OutMsgInvalid = 0,
}
