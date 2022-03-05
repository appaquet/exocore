use std::{cell::Cell, cmp::Ordering, io::Write, iter::Peekable};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use exocore_chain::{block::Block, chain::ChainStore};
use exocore_protos::{
    generated::data_chain_capnp::chain_operation,
    prost::{Message, ProstTimestampExt},
    store::{CommittedEntityMutation, EntityMutation, OrderingValue},
};
use extsort::ExternalSorter;

use crate::{
    error::Error,
    local::mutation_index::{MutationMetadata, MutationType, PutTraitMetadata},
    ordering::OrderingValueWrapper,
};

use super::EntityAggregator;

pub struct ChainEntityMutationIterator<'s> {
    iter: Box<dyn Iterator<Item = SortableMutation> + 's>,
}

impl<'s> ChainEntityMutationIterator<'s> {
    pub fn new<S: ChainStore>(
        chain_store: &'s S,
    ) -> Result<ChainEntityMutationIterator<'s>, Error> {
        let mut error: Option<Error> = None;
        let has_error = Cell::new(false);
        let mutations = chain_store
            .blocks_iter(0)
            .take(1000)
            .take_while(|_| !has_error.get())
            .flat_map(|block| match extract_block_mutations(block) {
                Ok(mutations) => mutations,
                Err(err) => {
                    error = Some(err);
                    has_error.set(true);
                    vec![]
                }
            });

        let sorter = ExternalSorter::new();
        let sorted_mutations = sorter
            .sort_by(mutations, SortableMutation::compare)
            .map_err(|err| Error::Other(anyhow!("error sorting mutations: {}", err)))?;

        // since the sorter goes through all mutations, any error found decoding mutations will be found at this point
        if let Some(err) = error {
            return Err(err);
        }

        Ok(ChainEntityMutationIterator {
            iter: Box::new(sorted_mutations),
        })
    }
}

fn extract_block_mutations(
    block: Result<
        exocore_chain::block::DataBlock<exocore_chain::chain::ChainData>,
        exocore_chain::chain::Error,
    >,
) -> Result<Vec<SortableMutation>, Error> {
    let block = block?;

    let mut mutations = Vec::new();
    for operation in block.operations_iter()? {
        let operation_reader = operation.get_reader()?;

        // only export entry operations (actual data, not chain maintenance related
        // operations)
        let data = match operation_reader
            .get_operation()
            .which()
            .map_err(|err| Error::Serialization(err.into()))?
        {
            chain_operation::operation::Entry(entry) => entry.unwrap().get_data().unwrap(),
            _ => continue,
        };

        let entity_mutation = EntityMutation::decode(data).unwrap();
        mutations.push(SortableMutation {
            entity_mutation: CommittedEntityMutation {
                block_offset: block.offset(),
                operation_id: operation_reader.get_operation_id(),
                mutation: Some(entity_mutation),
            },
        });
    }

    Ok(mutations)
}

impl<'s> Iterator for ChainEntityMutationIterator<'s> {
    type Item = CommittedEntityMutation;

    fn next(&mut self) -> Option<Self::Item> {
        let mutation = self.iter.next()?;
        Some(mutation.entity_mutation)
    }
}

pub struct ChainEntityIterator<'s> {
    mutations: Peekable<ChainEntityMutationIterator<'s>>,
    buffer: Vec<MutationMetadata>,
}

impl<'s> ChainEntityIterator<'s> {
    pub fn new<S: ChainStore>(chain_store: &'s S) -> Result<ChainEntityIterator<'s>, Error> {
        Ok(ChainEntityIterator {
            mutations: ChainEntityMutationIterator::new(chain_store)?.peekable(),
            buffer: Vec::new(),
        })
    }
}

impl<'s> Iterator for ChainEntityIterator<'s> {
    type Item = EntityMutation;

    fn next(&mut self) -> Option<Self::Item> {
        self.buffer.clear();

        let entity_id = self.mutations.peek()?.mutation.as_ref()?.entity_id.clone();
        loop {
            let next = self.mutations.peek()?;
            let next_id = &next.mutation.as_ref()?.entity_id;
            if entity_id != *next_id {
                break;
            }

            let mutation = self.mutations.next()?;
            self.buffer
                .push(entity_to_mutation_metadata(&mutation).ok()??); // TODO: fix me, check error
        }

        let aggr = EntityAggregator::new(self.buffer.drain(..));

        None
    }
}

fn entity_to_mutation_metadata(
    committed_entity: &CommittedEntityMutation,
) -> Result<Option<MutationMetadata>, Error> {
    use exocore_protos::store::entity_mutation::Mutation;
    let mutation = committed_entity
        .mutation
        .as_ref()
        .ok_or_else(|| Error::Other(anyhow!("no entity mutation")))?;

    let mutation_type = mutation
        .mutation
        .as_ref()
        .ok_or_else(|| Error::Other(anyhow!("no entity mutation")))?;

    let metadata = match mutation_type {
        Mutation::PutTrait(put) => Some(put_trait_to_metadata(put, &committed_entity, mutation)?),
        Mutation::DeleteTrait(del) => Some(del_trait_to_metadata(&committed_entity, mutation, del)),
        Mutation::DeleteEntity(del) => {
            Some(del_entity_to_metadata(&committed_entity, mutation, del))
        }
        _ => None,
    };

    Ok(metadata)
}

// TODO: prevent cloning

fn put_trait_to_metadata(
    put: &exocore_protos::store::PutTraitMutation,
    committed_entity: &CommittedEntityMutation,
    mutation: &EntityMutation,
) -> Result<MutationMetadata, Error> {
    let trt = put
        .r#trait
        .as_ref()
        .ok_or_else(|| Error::Other(anyhow!("no trait in PutTrait mutation")))?;
    Ok(MutationMetadata {
        operation_id: committed_entity.operation_id,
        block_offset: Some(committed_entity.block_offset),
        entity_id: mutation.entity_id.clone(),
        mutation_type: MutationType::TraitPut(PutTraitMetadata {
            trait_id: trt.id.clone(),
            trait_type: None,
            creation_date: trt.creation_date.as_ref().map(|d| d.to_chrono_datetime()),
            modification_date: trt
                .modification_date
                .as_ref()
                .map(|d| d.to_chrono_datetime()),
            has_reference: false,
        }),
        sort_value: OrderingValueWrapper::default(),
    })
}

fn del_trait_to_metadata(
    committed_entity: &CommittedEntityMutation,
    mutation: &EntityMutation,
    del: &exocore_protos::store::DeleteTraitMutation,
) -> MutationMetadata {
    MutationMetadata {
        operation_id: committed_entity.operation_id,
        block_offset: Some(committed_entity.block_offset),
        entity_id: mutation.entity_id.clone(),
        mutation_type: MutationType::TraitTombstone(del.trait_id.clone()),
        sort_value: OrderingValueWrapper::default(),
    }
}

fn del_entity_to_metadata(
    committed_entity: &CommittedEntityMutation,
    mutation: &EntityMutation,
    del: &exocore_protos::store::DeleteEntityMutation,
) -> MutationMetadata {
    MutationMetadata {
        operation_id: committed_entity.operation_id,
        block_offset: Some(committed_entity.block_offset),
        entity_id: mutation.entity_id.clone(),
        mutation_type: MutationType::EntityTombstone,
        sort_value: OrderingValueWrapper::default(),
    }
}
struct SortableMutation {
    entity_mutation: CommittedEntityMutation,
}

impl extsort::Sortable for SortableMutation {
    fn encode<W: Write>(&self, writer: &mut W) {
        let mutation = self.entity_mutation.encode_to_vec();
        writer
            .write_u64::<LittleEndian>(mutation.len() as u64)
            .unwrap();
        writer.write_all(&mutation).unwrap();
    }

    fn decode<R: std::io::Read>(reader: &mut R) -> Option<Self> {
        let len = reader.read_u64::<LittleEndian>().unwrap();
        let mut data = vec![0; len as usize];
        reader.read_exact(&mut data).unwrap();

        Some(SortableMutation {
            entity_mutation: CommittedEntityMutation::decode(data.as_ref()).unwrap(),
        })
    }
}

impl SortableMutation {
    fn compare(a: &SortableMutation, b: &SortableMutation) -> Ordering {
        let a_entity = a.entity_mutation.mutation.as_ref().unwrap();
        let b_entity = b.entity_mutation.mutation.as_ref().unwrap();

        match a_entity.entity_id.cmp(&b_entity.entity_id) {
            Ordering::Less => return Ordering::Less,
            Ordering::Equal => (),
            Ordering::Greater => return Ordering::Greater,
        }

        match a
            .entity_mutation
            .block_offset
            .cmp(&b.entity_mutation.block_offset)
        {
            Ordering::Less => return Ordering::Less,
            Ordering::Equal => (),
            Ordering::Greater => return Ordering::Greater,
        }

        a.entity_mutation
            .operation_id
            .cmp(&b.entity_mutation.operation_id)
    }
}
