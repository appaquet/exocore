#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Manifest {
    #[prost(string, tag = "1")]
    pub name: std::string::String,
    #[prost(string, tag = "2")]
    pub public_key: std::string::String,
    #[prost(message, repeated, tag = "3")]
    pub schemas_fdset: ::std::vec::Vec<Schema>,
}
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Schema {
    #[prost(oneof = "schema::Schema", tags = "1, 2")]
    pub schema: ::std::option::Option<schema::Schema>,
}
pub mod schema {
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Schema {
        #[prost(string, tag = "1")]
        File(std::string::String),
        #[prost(bytes, tag = "2")]
        Bytes(std::vec::Vec<u8>),
    }
}
