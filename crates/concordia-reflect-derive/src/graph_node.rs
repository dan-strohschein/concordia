//! `#[graph_node(category = "..", pure)]` (M4 4.9): keeps a free function and adds
//! `<fn>_graph_node() -> concordia_reflect::GraphFnInfo` describing it for the graph node library.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, LitStr, Pat, ReturnType};

/// The attribute's arguments.
#[derive(Default)]
struct Args {
    category: Option<LitStr>,
    pure: bool,
}

pub fn expand(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut parsed = Args::default();
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("category") {
            let c: LitStr = meta.value()?.parse()?;
            if c.value().trim().is_empty() {
                return Err(meta.error("category must not be empty"));
            }
            parsed.category = Some(c);
            Ok(())
        } else if meta.path.is_ident("pure") {
            parsed.pure = true;
            Ok(())
        } else {
            Err(meta.error("unknown graph_node argument; expected `category = \"...\"` or `pure`"))
        }
    });
    syn::parse::Parser::parse2(parser, args)?;
    let func: ItemFn = syn::parse2(item)?;
    let category = parsed.category.ok_or_else(|| {
        syn::Error::new_spanned(&func.sig.ident, "graph_node needs `category = \"...\"`")
    })?;
    if !func.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.generics,
            "graph_node functions can't be generic",
        ));
    }
    let mut inputs = Vec::new();
    for arg in &func.sig.inputs {
        let FnArg::Typed(typed) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "graph_node functions take no `self`",
            ));
        };
        let Pat::Ident(name) = &*typed.pat else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "graph_node arguments must be plain names",
            ));
        };
        let pin = name.ident.to_string();
        let pin = pin.trim_start_matches('_');
        let ty = &typed.ty;
        inputs.push(quote! { (#pin, ::concordia_reflect::TypeRef::of::<#ty>()) });
    }
    let output = match &func.sig.output {
        ReturnType::Default => quote! { ::std::option::Option::None },
        ReturnType::Type(_, ty) => {
            quote! { ::std::option::Option::Some(::concordia_reflect::TypeRef::of::<#ty>()) }
        }
    };
    let name = func.sig.ident.to_string();
    let docs = crate::attrs::docs(&func.attrs);
    let pure = parsed.pure;
    let vis = &func.vis;
    let describe = format_ident!("{}_graph_node", func.sig.ident);
    let doc = format!("`{name}` described for the graph node library.");
    Ok(quote! {
        #func

        #[doc = #doc]
        #vis fn #describe() -> ::concordia_reflect::GraphFnInfo {
            ::concordia_reflect::GraphFnInfo {
                name: #name,
                category: #category,
                docs: #docs,
                pure: #pure,
                inputs: ::std::vec![#(#inputs),*],
                output: #output,
            }
        }
    })
}
