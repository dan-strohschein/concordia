//! Parsing of doc comments, `#[reflect(..)]` and the allowed subset of `#[serde(..)]`.

use quote::ToTokens;
use syn::{Attribute, Expr, ExprLit, Lit, LitStr, Meta, Token, meta::ParseNestedMeta};

/// Serde attributes that keep serde's JSON identical to the reflected shape.
const CONTAINER_SERDE_ALLOWED: &[&str] = &[
    "transparent",
    "deny_unknown_fields",
    "default",
    "bound",
    "crate",
    "expecting",
];
const FIELD_SERDE_ALLOWED: &[&str] = &["default", "skip", "skip_serializing_if", "bound"];
const VARIANT_SERDE_ALLOWED: &[&str] = &[];

/// Type-level attributes.
#[derive(Default)]
pub struct ContainerAttrs {
    pub name: Option<LitStr>,
    pub transparent: bool,
    pub deny_unknown: bool,
    pub serde_default: bool,
}

/// Field-level attributes.
#[derive(Default)]
pub struct FieldAttrs {
    pub default: Option<Expr>,
    pub range: Option<(f64, f64)>,
    pub skip: bool,
    pub serde_optional: bool,
}

/// Joins `///` lines: each line trimmed, blank lines kept, outer whitespace trimmed.
pub fn docs(attrs: &[Attribute]) -> String {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })
        .flat_map(|text| {
            text.split('\n')
                .map(|l| l.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    lines.join("\n").trim().to_owned()
}

/// Parses type-level `#[reflect]` and `#[serde]` attributes.
pub fn container(attrs: &[Attribute]) -> syn::Result<ContainerAttrs> {
    let mut out = ContainerAttrs::default();
    for attr in attrs {
        if attr.path().is_ident("reflect") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    out.name = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("unknown reflect attribute; expected `name = \"...\"`"))
                }
            })?;
        } else if attr.path().is_ident("serde") {
            parse_serde(attr, CONTAINER_SERDE_ALLOWED, |name| match name {
                "transparent" => out.transparent = true,
                "deny_unknown_fields" => out.deny_unknown = true,
                "default" => out.serde_default = true,
                _ => {}
            })?;
        }
    }
    Ok(out)
}

/// Parses field-level `#[reflect]` and `#[serde]` attributes.
pub fn field(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in attrs {
        if attr.path().is_ident("reflect") {
            attr.parse_nested_meta(|meta| parse_field_reflect(&meta, &mut out))?;
        } else if attr.path().is_ident("serde") {
            parse_serde(attr, FIELD_SERDE_ALLOWED, |name| match name {
                "skip" => out.skip = true,
                "default" | "skip_serializing_if" => out.serde_optional = true,
                _ => {}
            })?;
        }
    }
    Ok(out)
}

/// Rejects every `#[serde]` attribute on enum variants, and any `#[reflect]`.
pub fn variant(attrs: &[Attribute]) -> syn::Result<()> {
    for attr in attrs {
        if attr.path().is_ident("serde") {
            parse_serde(attr, VARIANT_SERDE_ALLOWED, |_| {})?;
        } else if attr.path().is_ident("reflect") {
            return Err(syn::Error::new_spanned(
                attr,
                "#[reflect] is not supported on enum variants",
            ));
        }
    }
    Ok(())
}

fn parse_field_reflect(meta: &ParseNestedMeta, out: &mut FieldAttrs) -> syn::Result<()> {
    if meta.path.is_ident("default") {
        out.default = Some(meta.value()?.parse()?);
        Ok(())
    } else if meta.path.is_ident("range") {
        out.range = Some(parse_range(meta)?);
        Ok(())
    } else {
        Err(meta.error(
            "unknown reflect attribute; expected `default = ...` or `range(min = .., max = ..)`",
        ))
    }
}

fn parse_range(meta: &ParseNestedMeta) -> syn::Result<(f64, f64)> {
    let (mut min, mut max) = (None, None);
    meta.parse_nested_meta(|inner| {
        let value = number(&inner.value()?.parse::<Expr>()?)?;
        match inner.path.get_ident().map(ToString::to_string).as_deref() {
            Some("min") => min = Some(value),
            Some("max") => max = Some(value),
            _ => return Err(inner.error("expected `min` or `max`")),
        }
        Ok(())
    })?;
    match (min, max) {
        (Some(min), Some(max)) if min <= max => Ok((min, max)),
        (Some(min), Some(max)) => {
            Err(meta.error(format!("range min ({min}) must not exceed max ({max})")))
        }
        _ => Err(meta.error("range needs both `min` and `max`")),
    }
}

/// A numeric literal, optionally negated.
fn number(expr: &Expr) -> syn::Result<f64> {
    let (negative, lit) = match expr {
        Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) => (true, &*u.expr),
        other => (false, other),
    };
    let value = match lit {
        Expr::Lit(ExprLit {
            lit: Lit::Float(f), ..
        }) => f.base10_parse::<f64>()?,
        Expr::Lit(ExprLit {
            lit: Lit::Int(i), ..
        }) => i.base10_parse::<f64>()?,
        _ => {
            return Err(syn::Error::new_spanned(
                expr,
                "range bounds must be numeric literals",
            ));
        }
    };
    Ok(if negative { -value } else { value })
}

fn parse_serde(
    attr: &Attribute,
    allowed: &[&str],
    mut on_allowed: impl FnMut(&str),
) -> syn::Result<()> {
    attr.parse_nested_meta(|meta| {
        let name = meta.path.to_token_stream().to_string();
        if !allowed.contains(&name.as_str()) {
            return Err(meta.error(format!(
                "#[serde({name})] is not supported with Reflect: serde renaming, tagging, flattening and \
                 custom (de)serializers change the wire shape; use #[reflect(name = \"...\")] on the type instead"
            )));
        }
        skip_value(&meta)?;
        on_allowed(&name);
        Ok(())
    })
}

/// Consumes `= literal` or `(...)` after an allowed serde attribute.
fn skip_value(meta: &ParseNestedMeta) -> syn::Result<()> {
    if meta.input.peek(Token![=]) {
        meta.value()?.parse::<Lit>()?;
    } else if meta.input.peek(syn::token::Paren) {
        meta.parse_nested_meta(|inner| skip_value(&inner))?;
    }
    Ok(())
}

/// True if the type is spelled `Option<..>` (serde treats missing `Option` fields as `None`).
pub fn is_option(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Option"),
        _ => false,
    }
}
