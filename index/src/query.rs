use crate::entities::entity::Entity;

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

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct WithTraitQuery {
    trait_name: String,
    trait_query: Option<Box<Query>>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct ConjunctionQuery {
    queries: Vec<Query>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct MatchQuery {
    query: String,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct SortToken(pub String);

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryPaging {
    from_token: Option<SortToken>,
    to_token: Option<SortToken>,
    count: u32,
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
    results: Vec<QueryResult>,
    total_estimated: u32,
    current_page: QueryPaging,
    next_page: Option<QueryPaging>,
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
    entity: Entity,
    // TODO: sortToken:
}

///
///
///
///
pub trait OldQuery {
    fn fields(&self) -> Vec<&str>;
    fn value(&self) -> &str;
}

pub struct FieldMatchQuery<'a> {
    pub field_name: &'a str,
    pub value: &'a str,
}

impl<'a> OldQuery for FieldMatchQuery<'a> {
    fn fields(&self) -> Vec<&str> {
        vec![self.field_name]
    }

    fn value(&self) -> &str {
        self.value
    }
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
            results: vec![QueryResult { entity }],
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
