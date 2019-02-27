
# Replication

We have 2 data structures to replicate:
* Chain: immutable collection of blocks that contain entries. Entries can be arbitrary data stored for the index layer,
         or metadata entries related to the chain.
         
* Pending store: transient store in which latest operations are aggregated, to eventually be added to the chain.


## Pending store replication

### Messages

* PendingSyncRequest(PendingSyncRange)
* PendingSyncResponse(PendingSyncRange)

#### Operation
* Entries related (pending entry id = entry id)
    * OperationEntryNew
* Block related (pending entry id = block id)
    * BlockPropose
    * BlockProposalSign
    * BlockProposalRefuse (can happen after sign if node detects anomaly or accepts a better block)
* Maintenance related
    * Pending store cleanup mark


### Block proposal
* One node propose a block into pending store. 

### Cleanup
* We should only cleanup if stuff were committed to the chain OR we got a refusal quorum (everybody refused something).
  
* If a node was offline and received data before s



## Chain replication

### Messages
* ChainSyncRequest

### Cleanup
* A node that has access to unencrypted data can decide to cleanup the chain by truncating it, after moving entries around.
  The process:
  * Iterate through old blocks
  * For each entry, check if it's an old version of an entry
  * If it's an old entry, add to pending
  * Once we have a part of a chain that contains only old versions, propose a chain truncation

## Exceptions
* A node has signature of other nodes on a block, and is about to send his signature, but then get partitioned.
  He's the only one with quorum, and adds to the block.

  Solutions:
  * He'll never be able to make progress since all other nodes will eventually timeout and commit another block.
    He'll have to truncate its chain once he's sync back.
    Cons: We may be losing? Not supposed, since they will still be in other node's pending

  * Two stage commit where nobody adds to the chain unless everybody has agreed that they have signatures.
    Cons: This adds latency and communication for nothing... And it's an never ending story.


## TODO
- [ ] What is the logic on who proposes
        * Needs to have full data access
        * Needs to be considered online by others for them to wait for its proposal
- [ ] Conditional entry: entry can be conditional on time, other entry commit, etc.