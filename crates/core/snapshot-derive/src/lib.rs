//! `#[derive(Encode, Decode)]` for `cw_snapshot`'s portable snapshot format.
//!
//! A struct's body is its fields in declaration order; an enum's is its variant's
//! number in declaration order, then that variant's fields. A variant a `#[cfg]` leaves
//! out is gone before a derive sees the enum, so builds with other features number the
//! variants after it otherwise: their schemas say so, and a reader converts by name. `Encode` also derives `Describe`: the field names and types, and
//! the variants' numbers and names, that a snapshot's schema carries so that another
//! build can find them by name.
//!
//! A field a snapshot may lack is one serde would fill when JSON lacks it: an `Option`
//! (read as `None`), a field with `#[serde(default)]` or `#[serde(default = "path")]`,
//! or any field of a struct with `#[serde(default)]` (read as that field of the
//! struct's `Default`). Any other field is required.
//!
//! A field serde never reads or writes (`#[serde(skip)]`, `skip_serializing`,
//! `skip_deserializing`) is not written either and decodes as serde would fill it
//! (`#[serde(default = "path")]`, or `Default`). Every other serde attribute is about
//! JSON's shape and means nothing here. `#[snapshot(skip)]` skips a field serde keeps,
//! `#[snapshot(with = "module")]` encodes it with `module::encode` / `module::decode`
//! and describes it with `module::describe`.
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
    /// `#[serde(default)]`: the type's `Default`.
    default_bare: bool,
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
                (Some("default"), Meta::Path(_)) if !ours => out.default_bare = true,
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

/// `#[serde(default)]` on the container: every field defaults from the type's own.
fn container_default(attrs: &[Attribute]) -> syn::Result<bool> {
    for attr in attrs {
        if attr.path().is_ident("snapshot") {
            return Err(syn::Error::new_spanned(attr, "unknown snapshot attribute"));
        }
        if !attr.path().is_ident("serde") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let metas = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
        if metas
            .iter()
            .any(|m| matches!(m, Meta::Path(p) if p.is_ident("default")))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_option(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) if p.qself.is_none() => {
            p.path.segments.last().is_some_and(|s| s.ident == "Option")
        }
        _ => false,
    }
}

fn cfgs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs.iter().filter(|a| a.path().is_ident("cfg")).collect()
}

struct Field {
    member: syn::Member,
    binding: syn::Ident,
    attrs: FieldAttrs,
    cfg: Tokens,
    ty: syn::Type,
    name: String,
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
                name: match &f.ident {
                    Some(ident) => ident.to_string(),
                    None => i.to_string(),
                },
                member,
                binding,
                attrs: field_attrs(&f.attrs)?,
                cfg: quote!(#(#cfg)*),
                ty: f.ty.clone(),
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

/// Statements pushing each written field's `FieldSpec` onto `__f`; `bounds` gains the
/// types a default is encoded as.
fn describe_fields(
    fields: &[Field],
    container_default: bool,
    bounds: &mut Vec<syn::WherePredicate>,
) -> Tokens {
    let pushes = fields.iter().filter(|f| !f.attrs.skip).map(|f| {
        let (name, ty, cfg, m) = (&f.name, &f.ty, &f.cfg, &f.member);
        let describe = match &f.attrs.with {
            Some(with) => quote!(#with::describe),
            None => quote!(<#ty as ::cw_snapshot::Describe>::describe),
        };
        let encode = match &f.attrs.with {
            Some(with) => quote!(#with::encode),
            None => quote!(::cw_snapshot::Encode::encode),
        };
        let encoded = f.attrs.with.is_none()
            && (f.attrs.default.is_some() || f.attrs.default_bare || container_default);
        if encoded {
            bounds.push(syn::parse_quote!(#ty: ::cw_snapshot::Encode));
        }
        let default = if let Some(path) = &f.attrs.default {
            quote!(::core::option::Option::Some(|__w: &mut ::cw_snapshot::Writer| #encode(&#path(), __w)))
        } else if f.attrs.default_bare {
            quote!(::core::option::Option::Some(|__w: &mut ::cw_snapshot::Writer| #encode(&<#ty as ::core::default::Default>::default(), __w)))
        } else if container_default {
            quote!(::core::option::Option::Some(|__w: &mut ::cw_snapshot::Writer| #encode(&<Self as ::core::default::Default>::default().#m, __w)))
        } else if is_option(ty) && f.attrs.with.is_none() {
            quote!(::core::option::Option::Some(|__w: &mut ::cw_snapshot::Writer| __w.byte(0)))
        } else {
            quote!(::core::option::Option::None)
        };
        quote! {
            #cfg __f.push(::cw_snapshot::FieldSpec { name: #name, ty: #describe, default: #default });
        }
    });
    let pushes: Vec<Tokens> = pushes.collect();
    quote!(#(#pushes)*)
}

fn derive_describe(input: &DeriveInput) -> syn::Result<Tokens> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    for param in generics.params.iter_mut() {
        if let GenericParam::Type(t) = param {
            t.bounds.push(syn::parse_quote!(::cw_snapshot::Describe));
        }
    }
    let mut bounds = Vec::new();
    let body = match &input.data {
        Data::Struct(s) => {
            let defaults = container_default(&input.attrs)?;
            if defaults {
                let (_, ty, _) = input.generics.split_for_impl();
                bounds.push(syn::parse_quote!(#name #ty: ::core::default::Default));
            }
            let pushes = describe_fields(&fields(&s.fields)?, defaults, &mut bounds);
            quote! {
                let mut __f = ::std::vec::Vec::new();
                #pushes
                __s.record(__key, &__f)
            }
        }
        Data::Enum(e) => {
            let mut variants = Vec::new();
            for (index, v) in e.variants.iter().enumerate() {
                let cfg = cfgs(&v.attrs);
                let pushes = describe_fields(&fields(&v.fields)?, false, &mut bounds);
                let vname = v.ident.to_string();
                let index = index as u64;
                variants.push(quote! {
                    #(#cfg)* {
                        let mut __f = ::std::vec::Vec::new();
                        #pushes
                        __v.push(::cw_snapshot::VariantSpec { number: #index, name: #vname, fields: __f });
                    }
                });
            }
            quote! {
                let mut __v = ::std::vec::Vec::new();
                #(#variants)*
                __s.enumeration(__key, &__v)
            }
        }
        Data::Union(_) => unreachable!("refused by the encoder"),
    };
    // Only where the generic parameters reach: a bound on a concrete type is checked
    // where it is written, and would refuse a type that is not yet `Encode` there.
    let params: Vec<String> = input
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();
    let mentions = |p: &syn::WherePredicate| {
        let text = quote!(#p).to_string();
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| params.iter().any(|p| p == w))
    };
    {
        let wc = generics.make_where_clause();
        for b in bounds.into_iter().filter(mentions) {
            wc.predicates.push(b);
        }
    }
    let (imp, ty, wh) = generics.split_for_impl();
    Ok(quote! {
        impl #imp ::cw_snapshot::Describe for #name #ty #wh {
            #[allow(unused_mut, clippy::vec_init_then_push)]
            fn describe(__s: &mut ::cw_snapshot::Schema) -> ::cw_snapshot::Ty {
                let __key = ::core::any::type_name::<Self>();
                if let ::core::option::Option::Some(__t) = __s.known(__key) {
                    return __t;
                }
                #body
            }
        }
    })
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
            container_default(&input.attrs)?;
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
                let index = index as u64;
                if encode {
                    let pat = pattern(&path, &fields);
                    let writes = encode_fields(&fields);
                    arms.push(quote!(#(#cfg)* #pat => { __w.leb(#index); #writes }));
                } else {
                    let value = decode_fields(&path, &fields);
                    arms.push(quote!(#(#cfg)* #index => ::core::result::Result::Ok(#value),));
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
                quote! {
                    let __at = __r.position();
                    match __r.leb()? {
                        #(#arms)*
                        __other => ::core::result::Result::Err(::cw_snapshot::Error::at(__at, ::std::format!("no {} variant {}", #what, __other))),
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
    let describe = if encode {
        derive_describe(&input)?
    } else {
        quote!()
    };
    Ok(if encode {
        quote! {
            #describe
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
