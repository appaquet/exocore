use crate::domain::entity::{EntityId, Trait, TraitId};
use crate::error::Error;

#[serde(rename_all = "snake_case", tag = "type")]
#[derive(Serialize, Deserialize)]
pub enum Mutation {
    PutTrait(PutTraitMutation),
    DeleteTrait(DeleteTraitMutation),
}

impl Mutation {
    pub fn from_json(json: &str) -> Result<Mutation, Error> {
        serde_json::from_str(json).map_err(|err| err.into())
    }

    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string(self).map_err(|err| err.into())
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct PutTraitMutation {
    entity_id: EntityId,
    #[serde(rename = "trait")]
    trt: Trait,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct DeleteTraitMutation {
    entity_id: EntityId,
    trait_id: TraitId,
}
