@0xf7eba58b884a8143;

struct MutationRequest {
    request            @0: Data;
}

struct MutationResponse {
    operationId        @0: UInt64;
    error              @1: Text;
}

struct QueryRequest {
    request            @0: Data;
}

struct QueryResponse {
    response           @0: Data;
    error              @1: Text;
}
