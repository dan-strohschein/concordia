//! `Shape` generation for enums (serde's externally tagged form).

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DataEnum, Fields, Variant};

use crate::{attrs, fields};

/// The `Shape` expression for an enum.
pub fn shape(data: &DataEnum) -> syn::Result<TokenStream> {
    let variants = data
        .variants
        .iter()
        .map(variant)
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! { ::concordia_reflect::Shape::Enum { variants: ::std::vec![#(#variants),*] } })
}

fn variant(variant: &Variant) -> syn::Result<TokenStream> {
    attrs::variant(&variant.attrs)?;
    let name = variant.ident.to_string();
    let docs = attrs::docs(&variant.attrs);
    let shape = match &variant.fields {
        Fields::Unit => quote! { ::concordia_reflect::VariantShape::Unit },
        Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
            let inner = fields::type_ref(&unnamed.unnamed[0].ty);
            quote! { ::concordia_reflect::VariantShape::Newtype(#inner) }
        }
        Fields::Unnamed(unnamed) => {
            let items = fields::unnamed(unnamed)?;
            quote! { ::concordia_reflect::VariantShape::Tuple(#items) }
        }
        Fields::Named(named) => {
            let items = fields::named(named, false)?;
            quote! { ::concordia_reflect::VariantShape::Struct(#items) }
        }
    };
    Ok(quote! {
        ::concordia_reflect::VariantInfo { name: #name, docs: #docs, shape: #shape }
    })
}
