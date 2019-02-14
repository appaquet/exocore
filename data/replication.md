

# Replication order

* Chain
* Pending


## Pending store

### Messages
* New entry
    * Entry refusal ?
* Block related
    * Block propose
    * Block proposal sign
    * Block proposal sign cancel
    * Block proposal refuse

### Cleanup
* We should only cleanup if stuff were committed to the chain OR we got a refusal quorum (everybody refused something)


## Chain replication

Messages
* GetRange


## Exceptions
* A device has signature of other nodes on a block, and is about to send his signature but get partitionned.
  He's the only one with quorum, and adds to the block

  Solutions:
  * He'll never be able to make progress since all other nodes will eventually timeout and commit another block.
    He'll have to truncate its chain once he's sync back.

    Cons: We may be losing ?

  * Two stage commit where nobody adds to the chain unless everybody has agreed that they have signatures.
    This adds


## TODO
- [ ] Cleanup entry_id --> pending_id ?


