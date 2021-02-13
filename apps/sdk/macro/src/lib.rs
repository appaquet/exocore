use proc_macro::TokenStream;
use syn::{ ItemStruct, parse_macro_input};
use quote::quote;

#[proc_macro_attribute]
pub fn exocore_app(_metadata: TokenStream, input: TokenStream) -> TokenStream {
    let input_struct = parse_macro_input!(input as ItemStruct);
    let struct_ident = input_struct.ident.clone();

    TokenStream::from(quote!{
        #input_struct

        #[no_mangle]
        pub extern "C" fn __exocore_app_new() {
            let instance = <#struct_ident>::new();
            exocore_apps_sdk::__exocore_register_app(Box::new(instance));
        }
    })
}