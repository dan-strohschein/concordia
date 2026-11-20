//! Code generation for named fields and positional type lists.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::{Field, FieldsNamed, FieldsUnnamed};

use crate::attrs;

/// `vec![FieldInfo { .. }, ..]` for named fields (serde-skipped fields omitted).
pub fn named(fields: &FieldsNamed, all_optional: bool) -> syn::Result<TokenStream> {
    let infos = fields
        .named
        .iter()
        .filter_map(|f| field_info(f, all_optional).transpose())
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! { ::std::vec![#(#infos),*] })
}

/// `vec![TypeRef::of::<T>(), ..]` for positional fields (serde-skipped fields omitted).
pub fn unnamed(fields: &FieldsUnnamed) -> syn::Result<TokenStream> {
    let refs = kept(fields)?.into_iter().map(|f| type_ref(&f.ty));
    Ok(quote! { ::std::vec![#(#refs),*] })
}

/// Positional fields that serde serializes.
pub fn kept(fields: &FieldsUnnamed) -> syn::Result<Vec<&Field>> {
    let mut out = Vec::new();
    for field in &fields.unnamed {
        if !attrs::field(&field.attrs)?.skip {
            out.push(field);
        }
    }
    Ok(out)
}

/// `TypeRef::of::<T>()`.
pub fn type_ref(ty: &syn::Type) -> TokenStream {
    quote! { ::concordia_reflect::TypeRef::of::<#ty>() }
}

fn field_info(field: &Field, all_optional: bool) -> syn::Result<Option<TokenStream>> {
    let attrs = attrs::field(&field.attrs)?;
    if attrs.skip {
        return Ok(None);
    }
    let name = field
        .ident
        .as_ref()
        .map(|i| i.unraw().to_string())
        .unwrap_or_default();
    let ty = &field.ty;
    let ty_ref = type_ref(ty);
    let docs = attrs::docs(&field.attrs);
    let optional = all_optional || attrs.serde_optional || attrs::is_option(ty);
    let default = match &attrs.default {
        Some(expr) => quote! {
            ::std::option::Option::Some(::concordia_reflect::DefaultValue::Fn(|| {
                let value: #ty = #expr;
                ::concordia_reflect::__private::default_json(value)
            }))
        },
        None => quote! { ::std::option::Option::None },
    };
    let range = match attrs.range {
        Some((min, max)) => quote! {
            ::std::option::Option::Some(::concordia_reflect::Range { min: #min, max: #max })
        },
        None => quote! { ::std::option::Option::None },
    };
    let widget = match &attrs.widget {
        Some(w) => quote! { ::std::option::Option::Some(#w) },
        None => quote! { ::std::option::Option::None },
    };
    Ok(Some(quote! {
        ::concordia_reflect::FieldInfo {
            name: #name,
            ty: #ty_ref,
            docs: #docs,
            default: #default,
            range: #range,
            widget: #widget,
            optional: #optional,
        }
    }))
}
