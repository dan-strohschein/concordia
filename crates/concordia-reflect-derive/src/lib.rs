//! `#[derive(Reflect)]` for Concordia: generates a `TypeInfo` from a type's shape and doc comments.
//!
//! Use it through `concordia_reflect::Reflect`, not this crate directly.

mod attrs;
mod expand_enum;
mod expand_struct;
mod fields;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, parse_macro_input};

/// Derives `concordia_reflect::Reflect`. See the `concordia-reflect` crate docs.
#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Builder calls for the optional type-level `category` and `default`.
fn extras(container: &attrs::ContainerAttrs) -> proc_macro2::TokenStream {
    let category = container
        .category
        .as_ref()
        .map(|c| quote! { .with_category(#c) });
    let default = container.default.as_ref().map(|d| {
        let value = match d {
            attrs::ContainerDefault::Trait => {
                quote! { <Self as ::std::default::Default>::default() }
            }
            attrs::ContainerDefault::Expr(expr) => quote! { #expr },
        };
        quote! {
            .with_default(|| {
                let value: Self = #value;
                ::concordia_reflect::__private::default_json(value)
            })
        }
    });
    quote! { #category #default }
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Reflect does not support generic types in M0",
        ));
    }
    let container = attrs::container(&input.attrs)?;
    let shape = match &input.data {
        Data::Struct(data) => expand_struct::shape(data, &container)?,
        Data::Enum(data) => expand_enum::shape(data)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "Reflect does not support unions; use a struct or an enum",
            ));
        }
    };
    let ident = &input.ident;
    let name = container
        .name
        .as_ref()
        .map_or_else(|| ident.to_string(), syn::LitStr::value);
    let docs = attrs::docs(&input.attrs);
    let extras = extras(&container);
    Ok(quote! {
        impl ::concordia_reflect::Reflect for #ident {
            fn type_info() -> &'static ::concordia_reflect::TypeInfo {
                static INFO: ::std::sync::OnceLock<::concordia_reflect::TypeInfo> =
                    ::std::sync::OnceLock::new();
                INFO.get_or_init(|| {
                    ::concordia_reflect::TypeInfo::named(
                        #name,
                        ::std::any::type_name::<Self>(),
                        #docs,
                        #shape,
                    )
                    #extras
                })
            }
        }
    })
}
