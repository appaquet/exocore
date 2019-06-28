@0xfe4845b909a8f526;

struct Record {
    fields                  @0: List(Field);
}

struct Field {
    id                      @0: UInt16;
    value: union {
        string              @1: Text;
        numerical           @2: Int64;
    }
}
