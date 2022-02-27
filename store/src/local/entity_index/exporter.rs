use std::{cell::Cell, cmp::Ordering, io::Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use exocore_chain::{block::Block, chain::ChainStore};
use exocore_protos::{
    generated::data_chain_capnp::chain_operation,
    prost::Message,
    store::{CommittedEntityMutation, EntityMutation},
};
use extsort::ExternalSorter;

use crate::error::Error;

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
    mutation_iterator: ChainEntityMutationIterator<'s>,
}

impl<'s> ChainEntityIterator<'s> {
    pub fn new<S: ChainStore>(
        chain_store: &'s S,
    ) -> Result<ChainEntityMutationIterator<'s>, Error> {
        Ok(ChainEntityMutationIterator::new(chain_store)?)
    }
}

impl<'s> Iterator for ChainEntityIterator<'s> {
    type Item = EntityMutation;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
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
