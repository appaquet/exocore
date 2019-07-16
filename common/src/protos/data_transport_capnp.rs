#![allow(unknown_lints)]
#![allow(clippy::all)]

// Generated by the capnpc-rust plugin to the Cap'n Proto schema compiler.
// DO NOT EDIT.
// source: proto/data_transport.capnp

pub mod pending_sync_request {
    #[derive(Copy, Clone)]
    pub struct Owned;
    impl<'a> ::capnp::traits::Owned<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl<'a> ::capnp::traits::OwnedStruct<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl ::capnp::traits::Pipelined for Owned {
        type Pipeline = Pipeline;
    }

    #[derive(Clone, Copy)]
    pub struct Reader<'a> {
        reader: ::capnp::private::layout::StructReader<'a>,
    }

    impl<'a> ::capnp::traits::HasTypeId for Reader<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructReader<'a> for Reader<'a> {
        fn new(reader: ::capnp::private::layout::StructReader<'a>) -> Reader<'a> {
            Reader { reader: reader }
        }
    }

    impl<'a> ::capnp::traits::FromPointerReader<'a> for Reader<'a> {
        fn get_from_pointer(
            reader: &::capnp::private::layout::PointerReader<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Reader<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructReader::new(
                reader.get_struct(default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::IntoInternalStructReader<'a> for Reader<'a> {
        fn into_internal_struct_reader(self) -> ::capnp::private::layout::StructReader<'a> {
            self.reader
        }
    }

    impl<'a> ::capnp::traits::Imbue<'a> for Reader<'a> {
        fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {
            self.reader
                .imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))
        }
    }

    impl<'a> Reader<'a> {
        pub fn reborrow(&self) -> Reader {
            Reader { ..*self }
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.reader.total_size()
        }
        #[inline]
        pub fn get_ranges(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Reader<
                'a,
                crate::data_transport_capnp::pending_sync_range::Owned,
            >,
        > {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(0),
                ::std::option::Option::None,
            )
        }
        pub fn has_ranges(&self) -> bool {
            !self.reader.get_pointer_field(0).is_null()
        }
        #[inline]
        pub fn get_from_block_height(self) -> u64 {
            self.reader.get_data_field::<u64>(0)
        }
    }

    pub struct Builder<'a> {
        builder: ::capnp::private::layout::StructBuilder<'a>,
    }
    impl<'a> ::capnp::traits::HasStructSize for Builder<'a> {
        #[inline]
        fn struct_size() -> ::capnp::private::layout::StructSize {
            _private::STRUCT_SIZE
        }
    }
    impl<'a> ::capnp::traits::HasTypeId for Builder<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructBuilder<'a> for Builder<'a> {
        fn new(builder: ::capnp::private::layout::StructBuilder<'a>) -> Builder<'a> {
            Builder { builder: builder }
        }
    }

    impl<'a> ::capnp::traits::ImbueMut<'a> for Builder<'a> {
        fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
            self.builder
                .imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
        }
    }

    impl<'a> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a> {
        fn init_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            _size: u32,
        ) -> Builder<'a> {
            ::capnp::traits::FromStructBuilder::new(builder.init_struct(_private::STRUCT_SIZE))
        }
        fn get_from_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Builder<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructBuilder::new(
                builder.get_struct(_private::STRUCT_SIZE, default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::SetPointerBuilder<Builder<'a>> for Reader<'a> {
        fn set_pointer_builder<'b>(
            pointer: ::capnp::private::layout::PointerBuilder<'b>,
            value: Reader<'a>,
            canonicalize: bool,
        ) -> ::capnp::Result<()> {
            pointer.set_struct(&value.reader, canonicalize)
        }
    }

    impl<'a> Builder<'a> {
        pub fn into_reader(self) -> Reader<'a> {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }
        pub fn reborrow(&mut self) -> Builder {
            Builder { ..*self }
        }
        pub fn reborrow_as_reader(&self) -> Reader {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.builder.into_reader().total_size()
        }
        #[inline]
        pub fn get_ranges(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Builder<
                'a,
                crate::data_transport_capnp::pending_sync_range::Owned,
            >,
        > {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(0),
                ::std::option::Option::None,
            )
        }
        #[inline]
        pub fn set_ranges(
            &mut self,
            value: ::capnp::struct_list::Reader<
                'a,
                crate::data_transport_capnp::pending_sync_range::Owned,
            >,
        ) -> ::capnp::Result<()> {
            ::capnp::traits::SetPointerBuilder::set_pointer_builder(
                self.builder.get_pointer_field(0),
                value,
                false,
            )
        }
        #[inline]
        pub fn init_ranges(
            self,
            size: u32,
        ) -> ::capnp::struct_list::Builder<'a, crate::data_transport_capnp::pending_sync_range::Owned>
        {
            ::capnp::traits::FromPointerBuilder::init_pointer(
                self.builder.get_pointer_field(0),
                size,
            )
        }
        pub fn has_ranges(&self) -> bool {
            !self.builder.get_pointer_field(0).is_null()
        }
        #[inline]
        pub fn get_from_block_height(self) -> u64 {
            self.builder.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn set_from_block_height(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(0, value);
        }
    }

    pub struct Pipeline {
        _typeless: ::capnp::any_pointer::Pipeline,
    }
    impl ::capnp::capability::FromTypelessPipeline for Pipeline {
        fn new(typeless: ::capnp::any_pointer::Pipeline) -> Pipeline {
            Pipeline {
                _typeless: typeless,
            }
        }
    }
    impl Pipeline {}
    mod _private {
        use capnp::private::layout;
        pub const STRUCT_SIZE: layout::StructSize = layout::StructSize {
            data: 1,
            pointers: 1,
        };
        pub const TYPE_ID: u64 = 0xf23b_fe58_29c7_1a4b;
    }
}

pub mod pending_sync_range {
    #[derive(Copy, Clone)]
    pub struct Owned;
    impl<'a> ::capnp::traits::Owned<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl<'a> ::capnp::traits::OwnedStruct<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl ::capnp::traits::Pipelined for Owned {
        type Pipeline = Pipeline;
    }

    #[derive(Clone, Copy)]
    pub struct Reader<'a> {
        reader: ::capnp::private::layout::StructReader<'a>,
    }

    impl<'a> ::capnp::traits::HasTypeId for Reader<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructReader<'a> for Reader<'a> {
        fn new(reader: ::capnp::private::layout::StructReader<'a>) -> Reader<'a> {
            Reader { reader: reader }
        }
    }

    impl<'a> ::capnp::traits::FromPointerReader<'a> for Reader<'a> {
        fn get_from_pointer(
            reader: &::capnp::private::layout::PointerReader<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Reader<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructReader::new(
                reader.get_struct(default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::IntoInternalStructReader<'a> for Reader<'a> {
        fn into_internal_struct_reader(self) -> ::capnp::private::layout::StructReader<'a> {
            self.reader
        }
    }

    impl<'a> ::capnp::traits::Imbue<'a> for Reader<'a> {
        fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {
            self.reader
                .imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))
        }
    }

    impl<'a> Reader<'a> {
        pub fn reborrow(&self) -> Reader {
            Reader { ..*self }
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.reader.total_size()
        }
        #[inline]
        pub fn get_from_operation(self) -> u64 {
            self.reader.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn get_from_included(self) -> bool {
            self.reader.get_bool_field(64)
        }
        #[inline]
        pub fn get_to_operation(self) -> u64 {
            self.reader.get_data_field::<u64>(2)
        }
        #[inline]
        pub fn get_to_included(self) -> bool {
            self.reader.get_bool_field(65)
        }
        #[inline]
        pub fn get_operations_hash(self) -> ::capnp::Result<::capnp::data::Reader<'a>> {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(0),
                ::std::option::Option::None,
            )
        }
        pub fn has_operations_hash(&self) -> bool {
            !self.reader.get_pointer_field(0).is_null()
        }
        #[inline]
        pub fn get_operations_count(self) -> u32 {
            self.reader.get_data_field::<u32>(3)
        }
        #[inline]
        pub fn get_operations_frames(self) -> ::capnp::Result<::capnp::data_list::Reader<'a>> {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(1),
                ::std::option::Option::None,
            )
        }
        pub fn has_operations_frames(&self) -> bool {
            !self.reader.get_pointer_field(1).is_null()
        }
        #[inline]
        pub fn get_operations_headers(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Reader<
                'a,
                crate::data_chain_capnp::chain_operation_header::Owned,
            >,
        > {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(2),
                ::std::option::Option::None,
            )
        }
        pub fn has_operations_headers(&self) -> bool {
            !self.reader.get_pointer_field(2).is_null()
        }
    }

    pub struct Builder<'a> {
        builder: ::capnp::private::layout::StructBuilder<'a>,
    }
    impl<'a> ::capnp::traits::HasStructSize for Builder<'a> {
        #[inline]
        fn struct_size() -> ::capnp::private::layout::StructSize {
            _private::STRUCT_SIZE
        }
    }
    impl<'a> ::capnp::traits::HasTypeId for Builder<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructBuilder<'a> for Builder<'a> {
        fn new(builder: ::capnp::private::layout::StructBuilder<'a>) -> Builder<'a> {
            Builder { builder: builder }
        }
    }

    impl<'a> ::capnp::traits::ImbueMut<'a> for Builder<'a> {
        fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
            self.builder
                .imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
        }
    }

    impl<'a> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a> {
        fn init_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            _size: u32,
        ) -> Builder<'a> {
            ::capnp::traits::FromStructBuilder::new(builder.init_struct(_private::STRUCT_SIZE))
        }
        fn get_from_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Builder<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructBuilder::new(
                builder.get_struct(_private::STRUCT_SIZE, default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::SetPointerBuilder<Builder<'a>> for Reader<'a> {
        fn set_pointer_builder<'b>(
            pointer: ::capnp::private::layout::PointerBuilder<'b>,
            value: Reader<'a>,
            canonicalize: bool,
        ) -> ::capnp::Result<()> {
            pointer.set_struct(&value.reader, canonicalize)
        }
    }

    impl<'a> Builder<'a> {
        pub fn into_reader(self) -> Reader<'a> {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }
        pub fn reborrow(&mut self) -> Builder {
            Builder { ..*self }
        }
        pub fn reborrow_as_reader(&self) -> Reader {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.builder.into_reader().total_size()
        }
        #[inline]
        pub fn get_from_operation(self) -> u64 {
            self.builder.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn set_from_operation(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(0, value);
        }
        #[inline]
        pub fn get_from_included(self) -> bool {
            self.builder.get_bool_field(64)
        }
        #[inline]
        pub fn set_from_included(&mut self, value: bool) {
            self.builder.set_bool_field(64, value);
        }
        #[inline]
        pub fn get_to_operation(self) -> u64 {
            self.builder.get_data_field::<u64>(2)
        }
        #[inline]
        pub fn set_to_operation(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(2, value);
        }
        #[inline]
        pub fn get_to_included(self) -> bool {
            self.builder.get_bool_field(65)
        }
        #[inline]
        pub fn set_to_included(&mut self, value: bool) {
            self.builder.set_bool_field(65, value);
        }
        #[inline]
        pub fn get_operations_hash(self) -> ::capnp::Result<::capnp::data::Builder<'a>> {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(0),
                ::std::option::Option::None,
            )
        }
        #[inline]
        pub fn set_operations_hash(&mut self, value: ::capnp::data::Reader) {
            self.builder.get_pointer_field(0).set_data(value);
        }
        #[inline]
        pub fn init_operations_hash(self, size: u32) -> ::capnp::data::Builder<'a> {
            self.builder.get_pointer_field(0).init_data(size)
        }
        pub fn has_operations_hash(&self) -> bool {
            !self.builder.get_pointer_field(0).is_null()
        }
        #[inline]
        pub fn get_operations_count(self) -> u32 {
            self.builder.get_data_field::<u32>(3)
        }
        #[inline]
        pub fn set_operations_count(&mut self, value: u32) {
            self.builder.set_data_field::<u32>(3, value);
        }
        #[inline]
        pub fn get_operations_frames(self) -> ::capnp::Result<::capnp::data_list::Builder<'a>> {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(1),
                ::std::option::Option::None,
            )
        }
        #[inline]
        pub fn set_operations_frames(
            &mut self,
            value: ::capnp::data_list::Reader<'a>,
        ) -> ::capnp::Result<()> {
            ::capnp::traits::SetPointerBuilder::set_pointer_builder(
                self.builder.get_pointer_field(1),
                value,
                false,
            )
        }
        #[inline]
        pub fn init_operations_frames(self, size: u32) -> ::capnp::data_list::Builder<'a> {
            ::capnp::traits::FromPointerBuilder::init_pointer(
                self.builder.get_pointer_field(1),
                size,
            )
        }
        pub fn has_operations_frames(&self) -> bool {
            !self.builder.get_pointer_field(1).is_null()
        }
        #[inline]
        pub fn get_operations_headers(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Builder<
                'a,
                crate::data_chain_capnp::chain_operation_header::Owned,
            >,
        > {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(2),
                ::std::option::Option::None,
            )
        }
        #[inline]
        pub fn set_operations_headers(
            &mut self,
            value: ::capnp::struct_list::Reader<
                'a,
                crate::data_chain_capnp::chain_operation_header::Owned,
            >,
        ) -> ::capnp::Result<()> {
            ::capnp::traits::SetPointerBuilder::set_pointer_builder(
                self.builder.get_pointer_field(2),
                value,
                false,
            )
        }
        #[inline]
        pub fn init_operations_headers(
            self,
            size: u32,
        ) -> ::capnp::struct_list::Builder<'a, crate::data_chain_capnp::chain_operation_header::Owned>
        {
            ::capnp::traits::FromPointerBuilder::init_pointer(
                self.builder.get_pointer_field(2),
                size,
            )
        }
        pub fn has_operations_headers(&self) -> bool {
            !self.builder.get_pointer_field(2).is_null()
        }
    }

    pub struct Pipeline {
        _typeless: ::capnp::any_pointer::Pipeline,
    }
    impl ::capnp::capability::FromTypelessPipeline for Pipeline {
        fn new(typeless: ::capnp::any_pointer::Pipeline) -> Pipeline {
            Pipeline {
                _typeless: typeless,
            }
        }
    }
    impl Pipeline {}
    mod _private {
        use capnp::private::layout;
        pub const STRUCT_SIZE: layout::StructSize = layout::StructSize {
            data: 3,
            pointers: 3,
        };
        pub const TYPE_ID: u64 = 0xfb10_27d7_6f28_4abd;
    }
}

pub mod chain_sync_request {
    #[derive(Copy, Clone)]
    pub struct Owned;
    impl<'a> ::capnp::traits::Owned<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl<'a> ::capnp::traits::OwnedStruct<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl ::capnp::traits::Pipelined for Owned {
        type Pipeline = Pipeline;
    }

    #[derive(Clone, Copy)]
    pub struct Reader<'a> {
        reader: ::capnp::private::layout::StructReader<'a>,
    }

    impl<'a> ::capnp::traits::HasTypeId for Reader<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructReader<'a> for Reader<'a> {
        fn new(reader: ::capnp::private::layout::StructReader<'a>) -> Reader<'a> {
            Reader { reader: reader }
        }
    }

    impl<'a> ::capnp::traits::FromPointerReader<'a> for Reader<'a> {
        fn get_from_pointer(
            reader: &::capnp::private::layout::PointerReader<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Reader<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructReader::new(
                reader.get_struct(default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::IntoInternalStructReader<'a> for Reader<'a> {
        fn into_internal_struct_reader(self) -> ::capnp::private::layout::StructReader<'a> {
            self.reader
        }
    }

    impl<'a> ::capnp::traits::Imbue<'a> for Reader<'a> {
        fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {
            self.reader
                .imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))
        }
    }

    impl<'a> Reader<'a> {
        pub fn reborrow(&self) -> Reader {
            Reader { ..*self }
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.reader.total_size()
        }
        #[inline]
        pub fn get_from_offset(self) -> u64 {
            self.reader.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn get_to_offset(self) -> u64 {
            self.reader.get_data_field::<u64>(1)
        }
        #[inline]
        pub fn get_requested_details(
            self,
        ) -> ::std::result::Result<
            crate::data_transport_capnp::chain_sync_request::RequestedDetails,
            ::capnp::NotInSchema,
        > {
            ::capnp::traits::FromU16::from_u16(self.reader.get_data_field::<u16>(8))
        }
    }

    pub struct Builder<'a> {
        builder: ::capnp::private::layout::StructBuilder<'a>,
    }
    impl<'a> ::capnp::traits::HasStructSize for Builder<'a> {
        #[inline]
        fn struct_size() -> ::capnp::private::layout::StructSize {
            _private::STRUCT_SIZE
        }
    }
    impl<'a> ::capnp::traits::HasTypeId for Builder<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructBuilder<'a> for Builder<'a> {
        fn new(builder: ::capnp::private::layout::StructBuilder<'a>) -> Builder<'a> {
            Builder { builder: builder }
        }
    }

    impl<'a> ::capnp::traits::ImbueMut<'a> for Builder<'a> {
        fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
            self.builder
                .imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
        }
    }

    impl<'a> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a> {
        fn init_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            _size: u32,
        ) -> Builder<'a> {
            ::capnp::traits::FromStructBuilder::new(builder.init_struct(_private::STRUCT_SIZE))
        }
        fn get_from_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Builder<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructBuilder::new(
                builder.get_struct(_private::STRUCT_SIZE, default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::SetPointerBuilder<Builder<'a>> for Reader<'a> {
        fn set_pointer_builder<'b>(
            pointer: ::capnp::private::layout::PointerBuilder<'b>,
            value: Reader<'a>,
            canonicalize: bool,
        ) -> ::capnp::Result<()> {
            pointer.set_struct(&value.reader, canonicalize)
        }
    }

    impl<'a> Builder<'a> {
        pub fn into_reader(self) -> Reader<'a> {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }
        pub fn reborrow(&mut self) -> Builder {
            Builder { ..*self }
        }
        pub fn reborrow_as_reader(&self) -> Reader {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.builder.into_reader().total_size()
        }
        #[inline]
        pub fn get_from_offset(self) -> u64 {
            self.builder.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn set_from_offset(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(0, value);
        }
        #[inline]
        pub fn get_to_offset(self) -> u64 {
            self.builder.get_data_field::<u64>(1)
        }
        #[inline]
        pub fn set_to_offset(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(1, value);
        }
        #[inline]
        pub fn get_requested_details(
            self,
        ) -> ::std::result::Result<
            crate::data_transport_capnp::chain_sync_request::RequestedDetails,
            ::capnp::NotInSchema,
        > {
            ::capnp::traits::FromU16::from_u16(self.builder.get_data_field::<u16>(8))
        }
        #[inline]
        pub fn set_requested_details(
            &mut self,
            value: crate::data_transport_capnp::chain_sync_request::RequestedDetails,
        ) {
            self.builder.set_data_field::<u16>(8, value as u16)
        }
    }

    pub struct Pipeline {
        _typeless: ::capnp::any_pointer::Pipeline,
    }
    impl ::capnp::capability::FromTypelessPipeline for Pipeline {
        fn new(typeless: ::capnp::any_pointer::Pipeline) -> Pipeline {
            Pipeline {
                _typeless: typeless,
            }
        }
    }
    impl Pipeline {}
    mod _private {
        use capnp::private::layout;
        pub const STRUCT_SIZE: layout::StructSize = layout::StructSize {
            data: 3,
            pointers: 0,
        };
        pub const TYPE_ID: u64 = 0xb664_cf5f_668b_294b;
    }

    #[repr(u16)]
    #[derive(Clone, Copy, PartialEq)]
    pub enum RequestedDetails {
        Headers = 0,
        Blocks = 1,
    }
    impl ::capnp::traits::FromU16 for RequestedDetails {
        #[inline]
        fn from_u16(value: u16) -> ::std::result::Result<RequestedDetails, ::capnp::NotInSchema> {
            match value {
                0 => ::std::result::Result::Ok(RequestedDetails::Headers),
                1 => ::std::result::Result::Ok(RequestedDetails::Blocks),
                n => ::std::result::Result::Err(::capnp::NotInSchema(n)),
            }
        }
    }
    impl ::capnp::traits::ToU16 for RequestedDetails {
        #[inline]
        fn to_u16(self) -> u16 {
            self as u16
        }
    }
    impl ::capnp::traits::HasTypeId for RequestedDetails {
        #[inline]
        fn type_id() -> u64 {
            0xc5cf_bd89_6083_c936u64
        }
    }
}

pub mod chain_sync_response {
    #[derive(Copy, Clone)]
    pub struct Owned;
    impl<'a> ::capnp::traits::Owned<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl<'a> ::capnp::traits::OwnedStruct<'a> for Owned {
        type Reader = Reader<'a>;
        type Builder = Builder<'a>;
    }
    impl ::capnp::traits::Pipelined for Owned {
        type Pipeline = Pipeline;
    }

    #[derive(Clone, Copy)]
    pub struct Reader<'a> {
        reader: ::capnp::private::layout::StructReader<'a>,
    }

    impl<'a> ::capnp::traits::HasTypeId for Reader<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructReader<'a> for Reader<'a> {
        fn new(reader: ::capnp::private::layout::StructReader<'a>) -> Reader<'a> {
            Reader { reader: reader }
        }
    }

    impl<'a> ::capnp::traits::FromPointerReader<'a> for Reader<'a> {
        fn get_from_pointer(
            reader: &::capnp::private::layout::PointerReader<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Reader<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructReader::new(
                reader.get_struct(default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::IntoInternalStructReader<'a> for Reader<'a> {
        fn into_internal_struct_reader(self) -> ::capnp::private::layout::StructReader<'a> {
            self.reader
        }
    }

    impl<'a> ::capnp::traits::Imbue<'a> for Reader<'a> {
        fn imbue(&mut self, cap_table: &'a ::capnp::private::layout::CapTable) {
            self.reader
                .imbue(::capnp::private::layout::CapTableReader::Plain(cap_table))
        }
    }

    impl<'a> Reader<'a> {
        pub fn reborrow(&self) -> Reader {
            Reader { ..*self }
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.reader.total_size()
        }
        #[inline]
        pub fn get_from_offset(self) -> u64 {
            self.reader.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn get_to_offset(self) -> u64 {
            self.reader.get_data_field::<u64>(1)
        }
        #[inline]
        pub fn get_headers(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Reader<'a, crate::data_chain_capnp::block_partial_header::Owned>,
        > {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(0),
                ::std::option::Option::None,
            )
        }
        pub fn has_headers(&self) -> bool {
            !self.reader.get_pointer_field(0).is_null()
        }
        #[inline]
        pub fn get_blocks(self) -> ::capnp::Result<::capnp::data_list::Reader<'a>> {
            ::capnp::traits::FromPointerReader::get_from_pointer(
                &self.reader.get_pointer_field(1),
                ::std::option::Option::None,
            )
        }
        pub fn has_blocks(&self) -> bool {
            !self.reader.get_pointer_field(1).is_null()
        }
    }

    pub struct Builder<'a> {
        builder: ::capnp::private::layout::StructBuilder<'a>,
    }
    impl<'a> ::capnp::traits::HasStructSize for Builder<'a> {
        #[inline]
        fn struct_size() -> ::capnp::private::layout::StructSize {
            _private::STRUCT_SIZE
        }
    }
    impl<'a> ::capnp::traits::HasTypeId for Builder<'a> {
        #[inline]
        fn type_id() -> u64 {
            _private::TYPE_ID
        }
    }
    impl<'a> ::capnp::traits::FromStructBuilder<'a> for Builder<'a> {
        fn new(builder: ::capnp::private::layout::StructBuilder<'a>) -> Builder<'a> {
            Builder { builder: builder }
        }
    }

    impl<'a> ::capnp::traits::ImbueMut<'a> for Builder<'a> {
        fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
            self.builder
                .imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
        }
    }

    impl<'a> ::capnp::traits::FromPointerBuilder<'a> for Builder<'a> {
        fn init_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            _size: u32,
        ) -> Builder<'a> {
            ::capnp::traits::FromStructBuilder::new(builder.init_struct(_private::STRUCT_SIZE))
        }
        fn get_from_pointer(
            builder: ::capnp::private::layout::PointerBuilder<'a>,
            default: ::std::option::Option<&'a [::capnp::Word]>,
        ) -> ::capnp::Result<Builder<'a>> {
            ::std::result::Result::Ok(::capnp::traits::FromStructBuilder::new(
                builder.get_struct(_private::STRUCT_SIZE, default)?,
            ))
        }
    }

    impl<'a> ::capnp::traits::SetPointerBuilder<Builder<'a>> for Reader<'a> {
        fn set_pointer_builder<'b>(
            pointer: ::capnp::private::layout::PointerBuilder<'b>,
            value: Reader<'a>,
            canonicalize: bool,
        ) -> ::capnp::Result<()> {
            pointer.set_struct(&value.reader, canonicalize)
        }
    }

    impl<'a> Builder<'a> {
        pub fn into_reader(self) -> Reader<'a> {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }
        pub fn reborrow(&mut self) -> Builder {
            Builder { ..*self }
        }
        pub fn reborrow_as_reader(&self) -> Reader {
            ::capnp::traits::FromStructReader::new(self.builder.into_reader())
        }

        pub fn total_size(&self) -> ::capnp::Result<::capnp::MessageSize> {
            self.builder.into_reader().total_size()
        }
        #[inline]
        pub fn get_from_offset(self) -> u64 {
            self.builder.get_data_field::<u64>(0)
        }
        #[inline]
        pub fn set_from_offset(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(0, value);
        }
        #[inline]
        pub fn get_to_offset(self) -> u64 {
            self.builder.get_data_field::<u64>(1)
        }
        #[inline]
        pub fn set_to_offset(&mut self, value: u64) {
            self.builder.set_data_field::<u64>(1, value);
        }
        #[inline]
        pub fn get_headers(
            self,
        ) -> ::capnp::Result<
            ::capnp::struct_list::Builder<'a, crate::data_chain_capnp::block_partial_header::Owned>,
        > {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(0),
                ::std::option::Option::None,
            )
        }
        #[inline]
        pub fn set_headers(
            &mut self,
            value: ::capnp::struct_list::Reader<
                'a,
                crate::data_chain_capnp::block_partial_header::Owned,
            >,
        ) -> ::capnp::Result<()> {
            ::capnp::traits::SetPointerBuilder::set_pointer_builder(
                self.builder.get_pointer_field(0),
                value,
                false,
            )
        }
        #[inline]
        pub fn init_headers(
            self,
            size: u32,
        ) -> ::capnp::struct_list::Builder<'a, crate::data_chain_capnp::block_partial_header::Owned>
        {
            ::capnp::traits::FromPointerBuilder::init_pointer(
                self.builder.get_pointer_field(0),
                size,
            )
        }
        pub fn has_headers(&self) -> bool {
            !self.builder.get_pointer_field(0).is_null()
        }
        #[inline]
        pub fn get_blocks(self) -> ::capnp::Result<::capnp::data_list::Builder<'a>> {
            ::capnp::traits::FromPointerBuilder::get_from_pointer(
                self.builder.get_pointer_field(1),
                ::std::option::Option::None,
            )
        }
        #[inline]
        pub fn set_blocks(&mut self, value: ::capnp::data_list::Reader<'a>) -> ::capnp::Result<()> {
            ::capnp::traits::SetPointerBuilder::set_pointer_builder(
                self.builder.get_pointer_field(1),
                value,
                false,
            )
        }
        #[inline]
        pub fn init_blocks(self, size: u32) -> ::capnp::data_list::Builder<'a> {
            ::capnp::traits::FromPointerBuilder::init_pointer(
                self.builder.get_pointer_field(1),
                size,
            )
        }
        pub fn has_blocks(&self) -> bool {
            !self.builder.get_pointer_field(1).is_null()
        }
    }

    pub struct Pipeline {
        _typeless: ::capnp::any_pointer::Pipeline,
    }
    impl ::capnp::capability::FromTypelessPipeline for Pipeline {
        fn new(typeless: ::capnp::any_pointer::Pipeline) -> Pipeline {
            Pipeline {
                _typeless: typeless,
            }
        }
    }
    impl Pipeline {}
    mod _private {
        use capnp::private::layout;
        pub const STRUCT_SIZE: layout::StructSize = layout::StructSize {
            data: 2,
            pointers: 2,
        };
        pub const TYPE_ID: u64 = 0xb729_d9b2_39e7_4f27;
    }
}
