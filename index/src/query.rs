use crate::error::Error;
use chrono::{DateTime, Utc};
use exocore_common::framing::{CapnpFrameBuilder, FrameReader, TypedCapnpFrame};
use exocore_common::protos::index_transport_capnp::{
    query_request, query_response, watched_query_request,
};
use exocore_common::time::ConsistentTimestamp;
use exocore_schema::entity::{Entity, EntityId};
use exocore_schema::schema::Schema;
use exocore_schema::serialization::with_schema;
use std::collections::hash_map::DefaultHasher;
use std::sync::Arc;

pub type WatchToken = ConsistentTimestamp;
pub type ResultHash = u64;

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct Query {
    #[serde(flatten)]
    pub inner: InnerQuery,

    pub paging: Option<QueryPaging>,

    /// If true, only return summary
    pub summary: bool,

    /// If specified, if results from server matches this hash, only a summary will be returned.
    #[serde(skip)]
    pub result_hash: Option<ResultHash>,

    /// Optional watch token if this query is to be used for watching.
    pub watch_token: Option<WatchToken>,
}

#[serde(rename_all = "snake_case", tag = "type")]
#[derive(Serialize, Deserialize, Clone)]
pub enum InnerQuery {
    WithTrait(WithTraitQuery),
    Match(MatchQuery),
    IdEqual(IdEqualQuery),
    #[cfg(test)]
    TestFail(TestFailQuery),
}

impl Query {
    pub fn match_text<S: Into<String>>(query: S) -> Query {
        Query {
            inner: InnerQuery::Match(MatchQuery {
                query: query.into(),
            }),
            paging: None,
            summary: false,
            result_hash: None,
            watch_token: None,
        }
    }

    pub fn with_trait<S: Into<String>>(trait_name: S) -> Query {
        Query {
            inner: InnerQuery::WithTrait(WithTraitQuery {
                trait_name: trait_name.into(),
                trait_query: None,
            }),
            paging: None,
            summary: false,
            result_hash: None,
            watch_token: None,
        }
    }

    pub fn with_entity_id<S: Into<String>>(entity_id: S) -> Query {
        Query {
            inner: InnerQuery::IdEqual(IdEqualQuery {
                entity_id: entity_id.into(),
            }),
            paging: None,
            summary: false,
            result_hash: None,
            watch_token: None,
        }
    }

    #[cfg(test)]
    pub fn test_fail() -> Query {
        Query {
            inner: InnerQuery::TestFail(TestFailQuery {}),
            paging: None,
            summary: false,
            result_hash: None,
            watch_token: None,
        }
    }

    pub fn with_paging(mut self, paging: QueryPaging) -> Self {
        self.paging = Some(paging);
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        match self.paging.as_mut() {
            Some(paging) => paging.count = count,
            None => self.paging = Some(QueryPaging::new(count)),
        }

        self
    }

    pub fn only_summary(mut self) -> Self {
        self.summary = true;
        self
    }

    pub fn only_summary_if_equals(mut self, result_hash: ResultHash) -> Self {
        self.result_hash = Some(result_hash);
        self
    }

    pub fn paging_or_default(&self) -> &QueryPaging {
        self.paging.as_ref().unwrap_or(&QueryPaging::DEFAULT_PAGING)
    }

    pub fn with_watch_token(mut self, token: WatchToken) -> Self {
        self.watch_token = Some(token);
        self
    }

    pub fn to_request_frame(
        &self,
        schema: &Arc<Schema>,
    ) -> Result<CapnpFrameBuilder<query_request::Owned>, Error> {
        let mut frame_builder = CapnpFrameBuilder::<query_request::Owned>::new();
        let mut msg_builder = frame_builder.get_builder();
        let serialized_query = with_schema(schema, || serde_json::to_vec(&self))?;
        msg_builder.set_request(&serialized_query);

        Ok(frame_builder)
    }

    pub fn from_request_frame<I>(
        schema: &Arc<Schema>,
        frame: TypedCapnpFrame<I, query_request::Owned>,
    ) -> Result<Query, Error>
    where
        I: FrameReader,
    {
        let reader = frame.get_reader()?;
        let data = reader.get_request()?;
        let query = with_schema(schema, || serde_json::from_slice(data))?;

        Ok(query)
    }
}

/// Query that will be watched for changes and be consumed as a stream instead of a future.
#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct WatchedQuery {
    pub query: Query,
    pub token: WatchToken,
}

impl WatchedQuery {
    pub fn new(query: Query, token: WatchToken) -> WatchedQuery {
        WatchedQuery { query, token }
    }

    pub fn to_request_frame(
        &self,
        schema: &Arc<Schema>,
    ) -> Result<CapnpFrameBuilder<watched_query_request::Owned>, Error> {
        let mut frame_builder = CapnpFrameBuilder::<watched_query_request::Owned>::new();
        let mut msg_builder = frame_builder.get_builder();
        let serialized_query = with_schema(schema, || serde_json::to_vec(&self))?;
        msg_builder.set_request(&serialized_query);

        Ok(frame_builder)
    }

    pub fn from_request_frame<I>(
        schema: &Arc<Schema>,
        frame: TypedCapnpFrame<I, watched_query_request::Owned>,
    ) -> Result<WatchedQuery, Error>
    where
        I: FrameReader,
    {
        let reader = frame.get_reader()?;
        let data = reader.get_request()?;
        let query = with_schema(schema, || serde_json::from_slice(data))?;

        Ok(query)
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct WithTraitQuery {
    pub trait_name: String,
    pub trait_query: Option<Box<Query>>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct ConjunctionQuery {
    pub queries: Vec<Query>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct MatchQuery {
    pub query: String,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct IdEqualQuery {
    pub entity_id: EntityId,
}

#[cfg(test)]
#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Clone)]
pub struct TestFailQuery {}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QueryPaging {
    pub after_token: Option<SortToken>,
    pub before_token: Option<SortToken>,
    pub count: u32,
}

impl QueryPaging {
    pub const DEFAULT_PAGING: QueryPaging = QueryPaging {
        after_token: None,
        before_token: None,
        count: 10,
    };

    pub fn new(count: u32) -> QueryPaging {
        QueryPaging {
            after_token: None,
            before_token: None,
            count,
        }
    }

    pub fn with_after_token(mut self, token: SortToken) -> Self {
        self.after_token = Some(token);
        self
    }

    pub fn with_before_token(mut self, token: SortToken) -> Self {
        self.before_token = Some(token);
        self
    }

    pub fn is_sort_token_in_bound(&self, token: &SortToken) -> bool {
        if let Some(after_token) = self.after_token.as_ref() {
            if token <= after_token {
                return false;
            }
        }

        if let Some(before_token) = self.before_token.as_ref() {
            if token >= before_token {
                return false;
            }
        }

        true
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct SortToken(pub String);

impl SortToken {
    pub fn from_u64(value: u64) -> SortToken {
        format!("{:0>32x}", value).into()
    }

    pub fn to_u64(&self) -> Result<u64, Error> {
        let trimmed = self.0.trim_start_matches('0');
        if trimmed.is_empty() {
            Ok(0)
        } else {
            u64::from_str_radix(&self.0, 16).map_err(|err| {
                Error::QueryParsing(format!("Couldn't parse sort token from radix 36: {}", err))
            })
        }
    }

    pub fn from_datetime(value: DateTime<Utc>) -> SortToken {
        Self::from_u64(value.timestamp_nanos() as u64)
    }

    pub fn from_f32(value: f32) -> SortToken {
        format!("{}", value).into()
    }

    pub fn to_f32(&self) -> Result<f32, Error> {
        self.0.parse::<f32>().map_err(|err| {
            Error::QueryParsing(format!("Couldn't parse sort token to f32: {}", err))
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SortToken {
    fn from(value: String) -> Self {
        SortToken(value)
    }
}

///
/// Result of the query executed on index
///
#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug)]
pub struct QueryResult {
    pub results: Vec<EntityResult>,
    pub summary: bool,
    pub total_estimated: u32,
    pub current_page: QueryPaging,
    pub next_page: Option<QueryPaging>,
    pub hash: ResultHash,
}

impl QueryResult {
    pub fn empty() -> QueryResult {
        QueryResult {
            results: vec![],
            summary: false,
            total_estimated: 0,
            current_page: QueryPaging::new(0),
            next_page: None,
            hash: 0,
        }
    }

    pub fn result_to_response_frame(
        schema: &Arc<Schema>,
        result: Result<QueryResult, Error>,
    ) -> Result<CapnpFrameBuilder<query_response::Owned>, Error> {
        let mut frame_builder = CapnpFrameBuilder::<query_response::Owned>::new();
        let mut msg_builder = frame_builder.get_builder();

        match result {
            Ok(res) => {
                let serialized = with_schema(schema, || serde_json::to_vec(&res))?;
                msg_builder.set_response(&serialized);
            }
            Err(err) => {
                msg_builder.set_error(&err.to_string());
            }
        }

        Ok(frame_builder)
    }

    pub fn from_query_frame<I>(
        schema: &Arc<Schema>,
        frame: TypedCapnpFrame<I, query_response::Owned>,
    ) -> Result<QueryResult, Error>
    where
        I: FrameReader,
    {
        let reader = frame.get_reader()?;
        if reader.has_error() {
            Err(Error::Remote(reader.get_error()?.to_owned()))
        } else {
            let data = reader.get_response()?;
            let query_result = with_schema(schema, || serde_json::from_slice(data))?;
            Ok(query_result)
        }
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug)]
pub struct EntityResult {
    pub entity: Entity,
    pub source: EntityResultSource,
    pub sort_token: SortToken,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum EntityResultSource {
    Pending,
    Chain,
}

pub(crate) fn result_hasher() -> impl std::hash::Hasher {
    // TODO: Switch to a guaranteed deterministic lightweight hasher
    DefaultHasher::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_token_score_conversation() -> Result<(), failure::Error> {
        assert_eq!(
            SortToken::from_u64(1).as_str(),
            "00000000000000000000000000000001"
        );
        assert_eq!(SortToken::from_u64(0).to_u64()?, 0);
        assert_eq!(SortToken::from_u64(1234).to_u64()?, 1234);

        assert!(SortToken::from_f32(2.233_112).to_f32()? - 2.233_112 < std::f32::EPSILON);

        Ok(())
    }
}
