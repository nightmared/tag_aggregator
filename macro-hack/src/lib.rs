#![feature(proc_macro_hygiene)]
extern crate proc_macro;

use syn;
use quote::quote;

use proc_macro::TokenStream;

#[proc_macro]
pub fn try_future(input: TokenStream) -> TokenStream {
    let ast: syn::Expr = syn::parse(input).unwrap();
    let gen = quote! {
        match #ast {
            Ok(v) => v,
            Err(e) => return Box::new(future::err(e.into()))
        }
    };
    gen.into()
}

