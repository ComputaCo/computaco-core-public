//! `#[derive(Encode, Decode)]` for `cw_snapshot`'s portable snapshot format.
//!
//! A struct is its fields in declaration order; an enum is its variant's number in
//! declaration order, then that variant's fields. An enum with a `#[cfg]` on any
//! variant (or `#[snapshot(by_name)]`) is written by the variant's name instead, so
//! that builds with different features agree on its bytes.
//!
//! A field serde never reads or writes (`#[serde(skip)]`, `skip_serializing`,
//! `skip_deserializing`) is not written either and decodes as serde would fill it
//! (`#[serde(default = "path")]`, or `Default`). Every other serde attribute is about
//! JSON's shape and means nothing here. `#[snapshot(skip)]` skips a field serde keeps,
//! `#[snapshot(with = "module")]` encodes it with `module::encode` / `module::decode`.
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, punctuated::Punctuated, Attribute, Data, DeriveInput, Expr, ExprLit, Fields,
    GenericParam, Lit, Meta, Token,
};

#[derive(Default)]
struct FieldAttrs {
    skip: bool,
    default: Option<syn::Path>,
    with: Option<syn::Path>,
}

fn lit_path(expr: &Expr) -> syn::Result<syn::Path> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => s.parse(),
        other => Err(syn::Error::new_spanned(other, "expected a string")),
    }
}

fn field_attrs(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in attrs {
        let ours = attr.path().is_ident("snapshot");
        if !ours && !attr.path().is_ident("serde") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let metas = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
        for meta in metas {
            let name = meta.path().get_ident().map(|i| i.to_string());
            match (name.as_deref(), &meta) {
                (Some("skip" | "skip_serializing" | "skip_deserializing"), Meta::Path(_)) => {
                    out.skip = true
                }
                (Some("default"), Meta::NameValue(nv)) if !ours => {
                    out.default = Some(lit_path(&nv.value)?)
                }
                (Some("with"), Meta::NameValue(nv)) if ours => {
                    out.with = Some(lit_path(&nv.value)?)
                }
                _ if ours => {
                    return Err(syn::Error::new_spanned(meta, "unknown snapshot attribute"))
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn by_name(attrs: &[Attribute]) -> syn::Result<bool> {
    let mut by_name = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("snapshot")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("by_name") {
                by_name = true;
                Ok(())
            } else {
                Err(meta.error("unknown snapshot attribute"))
            }
        })?;
    }
    Ok(by_name)
}

fn cfgs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs.iter().filter(|a| a.path().is_ident("cfg")).collect()
}

struct Field {
    member: syn::Member,
    binding: syn::Ident,
    attrs: FieldAttrs,
    cfg: Tokens,
}

fn fields(fields: &Fields) -> syn::Result<Vec<Field>> {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let member = match &f.ident {
                Some(ident) => syn::Member::Named(ident.clone()),
                None => syn::Member::Unnamed(i.into()),
            };
            let binding = match &f.ident {
                Some(ident) => format_ident!("__{}", ident),
                None => format_ident!("__{}", i),
            };
            let cfg = cfgs(&f.attrs);
            Ok(Field {
                member,
                binding,
                attrs: field_attrs(&f.attrs)?,
                cfg: quote!(#(#cfg)*),
            })
        })
        .collect()
}

/// `Self { a: __a, .. }` (or the variant's path) binding every written field.
fn pattern(path: &Tokens, fields: &[Field]) -> Tokens {
    let bound = fields.iter().filter(|f| !f.attrs.skip).map(|f| {
        let (m, b, cfg) = (&f.member, &f.binding, &f.cfg);
        quote!(#cfg #m: #b)
    });
    quote!(#path { #(#bound,)* .. })
}

fn encode_fields(fields: &[Field]) -> Tokens {
    let writes = fields.iter().filter(|f| !f.attrs.skip).map(|f| {
        let (b, cfg) = (&f.binding, &f.cfg);
        match &f.attrs.with {
            Some(with) => quote!(#cfg #with::encode(#b, __w);),
            None => quote!(#cfg ::cw_snapshot::Encode::encode(#b, __w);),
        }
    });
    quote!(#(#writes)*)
}

fn decode_fields(path: &Tokens, fields: &[Field]) -> Tokens {
    let reads = fields.iter().map(|f| {
        let (m, cfg) = (&f.member, &f.cfg);
        let value = if f.attrs.skip {
            match &f.attrs.default {
                Some(default) => quote!(#default()),
                None => quote!(::core::default::Default::default()),
            }
        } else {
            match &f.attrs.with {
                Some(with) => quote!(#with::decode(__r)?),
                None => quote!(::cw_snapshot::Decode::decode(__r)?),
            }
        };
        quote!(#cfg #m: #value)
    });
    quote!(#path { #(#reads,)* })
}

fn derive(input: DeriveInput, encode: bool) -> syn::Result<Tokens> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let bound: syn::Path = if encode {
        syn::parse_quote!(::cw_snapshot::Encode)
    } else {
        syn::parse_quote!(::cw_snapshot::Decode)
    };
    for param in generics.params.iter_mut() {
        if let GenericParam::Type(t) = param {
            t.bounds.push(syn::TypeParamBound::Trait(syn::TraitBound {
                paren_token: None,
                modifier: syn::TraitBoundModifier::None,
                lifetimes: None,
                path: bound.clone(),
            }));
        }
    }
    let (imp, ty, wh) = generics.split_for_impl();
    let body = match &input.data {
        Data::Struct(s) => {
            let fields = fields(&s.fields)?;
            if encode {
                let pat = pattern(&quote!(Self), &fields);
                let writes = encode_fields(&fields);
                quote! { let #pat = self; #writes }
            } else {
                let value = decode_fields(&quote!(Self), &fields);
                quote! { ::core::result::Result::Ok(#value) }
            }
        }
        Data::Enum(e) => {
            let named =
                by_name(&input.attrs)? || e.variants.iter().any(|v| !cfgs(&v.attrs).is_empty());
            let mut arms = Vec::new();
            for (index, v) in e.variants.iter().enumerate() {
                if field_attrs(&v.attrs)?.skip {
                    return Err(syn::Error::new_spanned(
                        v,
                        "a skipped variant cannot be in a snapshot",
                    ));
                }
                let vname = &v.ident;
                let cfg = cfgs(&v.attrs);
                let fields = fields(&v.fields)?;
                let path = quote!(Self::#vname);
                let label = vname.to_string();
                let index = index as u64;
                if encode {
                    let pat = pattern(&path, &fields);
                    let writes = encode_fields(&fields);
                    let tag = if named {
                        quote!(__w.str(#label);)
                    } else {
                        quote!(__w.leb(#index);)
                    };
                    arms.push(quote!(#(#cfg)* #pat => { #tag #writes }));
                } else {
                    let value = decode_fields(&path, &fields);
                    let key = if named {
                        quote!(#label)
                    } else {
                        quote!(#index)
                    };
                    arms.push(quote!(#(#cfg)* #key => ::core::result::Result::Ok(#value),));
                }
            }
            if encode {
                if arms.is_empty() {
                    quote!(match *self {})
                } else {
                    quote!(match self { #(#arms)* })
                }
            } else {
                let what = name.to_string();
                if named {
                    quote! {
                        let __at = __r.position();
                        match __r.str()? {
                            #(#arms)*
                            __other => ::core::result::Result::Err(::cw_snapshot::Error::at(__at, ::std::format!("no {} variant {:?}", #what, __other))),
                        }
                    }
                } else {
                    quote! {
                        let __at = __r.position();
                        match __r.leb()? {
                            #(#arms)*
                            __other => ::core::result::Result::Err(::cw_snapshot::Error::at(__at, ::std::format!("no {} variant {}", #what, __other))),
                        }
                    }
                }
            }
        }
        Data::Union(_) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "a union cannot be in a snapshot",
            ))
        }
    };
    Ok(if encode {
        quote! {
            impl #imp ::cw_snapshot::Encode for #name #ty #wh {
                #[allow(unused_variables, non_shorthand_field_patterns)]
                fn encode(&self, __w: &mut ::cw_snapshot::Writer) { #body }
            }
        }
    } else {
        quote! {
            impl #imp ::cw_snapshot::Decode for #name #ty #wh {
                #[allow(unused_variables)]
                fn decode(__r: &mut ::cw_snapshot::Reader<'_>) -> ::core::result::Result<Self, ::cw_snapshot::Error> { #body }
            }
        }
    })
}

#[proc_macro_derive(Encode, attributes(snapshot))]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive(input, true)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Decode, attributes(snapshot))]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive(input, false)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
