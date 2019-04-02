@0xf51296176d1e327e;

#
# Pending store
#
struct PendingOperation {
    groupId                @0: UInt64;
    operationId            @1: UInt64;
    nodeId                 @2: Text;

    operation :union {
        entryNew           @3: OperationEntryNew;
        blockPropose       @4: OperationBlockPropose;
        blockSign          @5: OperationBlockSign;
        blockRefuse        @6: OperationBlockRefuse;
        pendingIgnore      @7: OperationPendingIgnore;
    }
}

# Used by transport for pending synchronization
struct PendingOperationHeader {
    groupId                @0: UInt64;
    operationId            @1: UInt64;
    operationSignature     @2: Data;
}

struct OperationEntryNew {
    data                   @0: Data;
}

struct OperationBlockPropose {
    block                  @0: Data; # frame of type Block
}

struct OperationBlockSign {
    signature              @0: BlockSignature;
}

struct OperationBlockRefuse {
}

struct OperationPendingIgnore {
    groupId                @0: UInt64;
}

#
# Chain
#
struct Block { # Rename... It's not a block anymore, but a block header
    offset                 @0: UInt64;
    depth                  @1: UInt64;
    previousOffset         @2: UInt64;
    previousHash           @3: Data;
    proposedOperationId    @4: UInt64;
    proposedNodeId         @5: Text;

    entriesSize            @6: UInt32;  # Data size
    entriesHeader          @7: List(BlockEntryHeader);
    entriesHash            @8: Data;

    signaturesSize         @9: UInt16;
}

# Used by transport for chain synchronization
struct BlockHeader {
    offset                 @0: UInt64;
    depth                  @1: UInt64;
    previousOffset         @2: UInt64;
    previousHash           @3: Data;
    proposedOperationId    @4: UInt64;
    proposedNodeId         @5: Text;

    blockSize              @6: UInt32;
    blockHash              @7: Data;

    entriesSize            @8: UInt32;
    signaturesSize         @9: UInt16;
}

struct BlockEntryHeader {
    operationId            @0: UInt64;
    dataOffset             @1: UInt32;
    dataSize               @2: UInt32;
}

struct BlockSignatures {
    entriesSize            @0: UInt32;
    signatures             @1: List(BlockSignature);
}

# Represents signature of the Block's frame data
struct BlockSignature {
    nodeId                 @0: Text;
    nodeSignature          @1: Data;
}

