@0xc2b0b9df716f42ac;

using Chain = import "data_chain.capnp";
using Common = import "common.capnp";

struct Envelope {
    type @0: UInt8;
    from @1: Common.Node;
}

struct PendingSyncRequest {
    ranges @0: List(PendingSyncRange);
}

struct PendingSyncResponse {
    ranges @0: List(PendingSyncRange);
}

struct PendingSyncRange {
    fromTime @0: UInt64;
    toTime @1: UInt64;

    requestedDetails @2: RequestedDetails;

    hash @3: Data;

    # TODO: We don't want to send pending operations directly if we only asked for hash or header
    operations @4: List(Chain.PendingOperation);

    enum RequestedDetails {
      hash @0;
      headers @1;
      full @2;
    }
}

