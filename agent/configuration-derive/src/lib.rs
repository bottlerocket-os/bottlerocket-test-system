//! Automatically implement `Configuration` for an agent's config

use proc_macro::{self, TokenStream};

#[macro_use]
extern crate quote;
#[proc_macro_derive(Configuration, attributes(crd))]
pub fn derive_configuration(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast: syn::DeriveInput = syn::parse(input).unwrap();
    let ident = ast.ident;

    quote! {
       impl testsys_model::Configuration for #ident{}
    }
    .into()
}
