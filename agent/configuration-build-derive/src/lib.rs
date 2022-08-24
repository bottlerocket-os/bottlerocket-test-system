//! Create a builder for TestSys agent configurations
//!
//! Each field of the configuration is given a setter for a templated
//! value, or a typed value.

use crate::derive::{build_struct, impl_configuration};
use proc_macro::{self, TokenStream};
mod derive;

#[macro_use]
extern crate quote;
#[proc_macro_derive(Configuration, attributes(crd))]
pub fn derive_configuration(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast = syn::parse(input).unwrap();

    // impl Configuration
    let impl_conf = impl_configuration(&ast);

    // Create the builder struct
    let builder = build_struct(&ast);

    quote! {
        #impl_conf
        #builder
    }
    .into()
}
