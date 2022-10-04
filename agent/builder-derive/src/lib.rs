//! Create a builder for TestSys agent configurations
//!
//! Each field of the configuration is given a setter for a templated
//! value, or a typed value.
//!
//! Create a configuration struct and derive `Configuration`, the derive `Builder`.
//! The `crd` attribute must be set as either `Test` or `Resource`
//! A new builder can be created by calling `Config::builder()`
//! To create the test crd from the builder use `build(<NAME>)`
//! ```
//! use configuration_derive::Configuration;
//! use builder_derive::Builder;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
//! #[crd("Test")]
//! struct Config{
//!     field: Option<String>
//! }
//!
//! let test_crd = Config::builder().image("agent image").build("test name");
//! ```

use crate::derive::{build_struct, builder_fn};
use proc_macro::{self, TokenStream};
mod derive;

#[macro_use]
extern crate quote;
#[proc_macro_derive(Builder, attributes(crd))]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast = syn::parse(input).unwrap();

    // impl Configuration
    let builder_fn = builder_fn(&ast);

    // Create the builder struct
    let builder = build_struct(&ast);

    quote! {
        #builder_fn
        #builder
    }
    .into()
}
