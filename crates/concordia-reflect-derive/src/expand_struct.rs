//! `Shape` generation for structs.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DataStruct, Fields};

use crate::attrs::ContainerAttrs;
use crate::fields;

/// The `Shape` expression for a struct.
pub fn shape(data: &DataStruct, attrs: &ContainerAttrs) -> syn::Result<TokenStream> {
    match &data.fields {
        Fields::Named(named) if attrs.transparent => transparent_named(named),
        Fields::Named(named) => {
            let fields = fields::named(named, attrs.serde_default)?;
            let deny = attrs.deny_unknown;
            Ok(
                quote! { ::concordia_reflect::Shape::Struct { fields: #fields, deny_unknown: #deny } },
            )
        }
        Fields::Unnamed(unnamed) => match fields::kept(unnamed)?.as_slice() {
            [only] => Ok(newtype(&only.ty, attrs.transparent)),
            _ => {
                let fields = fields::unnamed(unnamed)?;
                Ok(quote! { ::concordia_reflect::Shape::TupleStruct { fields: #fields } })
            }
        },
        Fields::Unit => Ok(quote! { ::concordia_reflect::Shape::Unit }),
    }
}

fn transparent_named(named: &syn::FieldsNamed) -> syn::Result<TokenStream> {
    let mut kept = Vec::new();
    for field in &named.named {
        if !crate::attrs::field(&field.attrs)?.skip {
            kept.push(field);
        }
    }
    match kept.as_slice() {
        [only] => Ok(newtype(&only.ty, true)),
        _ => Err(syn::Error::new_spanned(
            &named.named,
            "#[serde(transparent)] needs exactly one serialized field",
        )),
    }
}

fn newtype(ty: &syn::Type, transparent: bool) -> TokenStream {
    let inner = fields::type_ref(ty);
    quote! { ::concordia_reflect::Shape::Newtype { inner: #inner, transparent: #transparent } }
}
