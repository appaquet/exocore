use crate::domain::entity::Entity;

///
///
///
#[serde(rename_all = "snake_case", tag = "type")]
#[derive(Serialize, Deserialize)]
pub enum Query {
    WithTrait(WithTraitQuery),
    Conjunction(ConjunctionQuery),
    Match(MatchQuery),

    // TODO: just for tests for now
    Empty,
}

impl Query {
    pub fn match_text<S: Into<String>>(query: S) -> Query {
        Query::Match(MatchQuery {
            query: query.into(),
        })
    }

    pub fn with_trait<S: Into<String>>(trait_name: S) -> Query {
        Query::WithTrait(WithTraitQuery {
            trait_name: trait_name.into(),
            trait_query: None,
        })
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct WithTraitQuery {
    pub trait_name: String,
    pub trait_query: Option<Box<Query>>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct ConjunctionQuery {
    pub queries: Vec<Query>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct MatchQuery {
    pub query: String,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct SortToken(pub String);

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryPaging {
    pub from_token: Option<SortToken>,
    pub to_token: Option<SortToken>,
    pub count: u32,
}
impl QueryPaging {
    fn empty() -> QueryPaging {
        QueryPaging {
            from_token: None,
            to_token: None,
            count: 0,
        }
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryResults {
    pub results: Vec<QueryResult>,
    pub total_estimated: u32,
    pub current_page: QueryPaging,
    pub next_page: Option<QueryPaging>,
    // TODO: currentPage, nextPage, queryToken
}

impl QueryResults {
    pub fn empty() -> QueryResults {
        QueryResults {
            results: vec![],
            total_estimated: 0,
            current_page: QueryPaging::empty(),
            next_page: None,
        }
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryResult {
    pub entity: Entity,
    pub source: QueryResultSource,
    // TODO: sortToken:
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum QueryResultSource {
    Pending,
    Chain,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() -> Result<(), failure::Error> {
        let query = Query::WithTrait(WithTraitQuery {
            trait_name: "trait".to_string(),
            trait_query: None,
        });
        let serialized = serde_json::to_string(&query)?;
        println!("{}", serialized);

        let entity = Entity::new("1234".to_string());
        let results = QueryResults {
            results: vec![QueryResult {
                entity,
                source: QueryResultSource::Pending,
            }],
            total_estimated: 0,
            current_page: QueryPaging {
                from_token: Some(SortToken("token".to_string())),
                to_token: None,
                count: 10,
            },
            next_page: None,
        };
        let serialized = serde_json::to_string(&results)?;
        println!("{}", serialized);

        Ok(())
    }

}
