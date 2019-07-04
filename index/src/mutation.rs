use crate::domain::entity::{EntityId, Trait};

#[serde(rename_all = "snake_case", tag = "type")]
#[derive(Serialize, Deserialize)]
pub enum Mutation {
    PutTrait(PutTraitMutation),
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct PutTraitMutation {
    entity_id: EntityId,
    #[serde(rename = "trait")]
    trt: Trait,
}
