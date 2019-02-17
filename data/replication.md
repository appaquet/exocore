

# Replication

We have 2 data structure to replicate:
* Chain
* Pending store


## Pending store replication

### Messages

* PendingSyncRequest(PendingSyncRange)
* PendingSyncResponse(PendingSyncRange)

### Operationdyn s
* New entry (pending entry id = entry id)
    * Entry refusal ?
* Block related (pending entry id = block id)
    * Block propose
    * Block proposal sign
    * Block proposal sign cancel
    * Block proposal refuse

### Cleanup
* We should only cleanup if stuff were committed to the chain OR we got a refusal quorum (everybody refused something)




## Chain replication

Messages
* DescribeRange
* GetRange

### Cleanup
* A node that has access to unencrypted data can decide to cleanup the chain by truncating it, after moving entries around.
  The process:
  * Iterate through old blocks
  * For each entry, check if it's an old version of an entry
  * If it's an old entry, add to pending
  * Once we have a part of a chain that contains only old versions, propose a chain truncation


## Exceptions
* A node has signature of other nodes on a block, and is about to send his signature, but then get partitionned.
  He's the only one with quorum, and adds to the block.

  Solutions:
  * He'll never be able to make progress since all other nodes will eventually timeout and commit another block.
    He'll have to truncate its chain once he's sync back.
    Cons: We may be losing? Not supposed, since they will still be in other node's pending

  * Two stage commit where nobody adds to the chain unless everybody has agreed that they have signatures.
    Cons: This adds latency and communication for nothing... And it's an never ending story.


## TODO
- [ ] Rename entry_id --> pending_group ? (first operation id)
- [ ] What is the logic on who proposes
        * Needs to have full data access
        * Needs to be considered online by others for them to wait for its proposal
- [ ] Add chain entry type
        * Block proposal
- [ ] Remove hash from capnp messages since they are now in the framing protocol
- [ ] Conditional entry: entry can be conditional on time, other entry commit, etc.