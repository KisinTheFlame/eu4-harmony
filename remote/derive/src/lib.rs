//! `#[derive(Shaped)]`: a type's shape (`eu4_harmony_remote::shape`), read
//! from its fields and the serde attributes that say how it is written:
//! `rename`, `rename_all`, `untagged`, `tag`, `transparent`, `flatten`,
//! `skip`, and `skip_serializing_if` (the field may be left out). Doc
//! comments become the shape's words. So the shape is the answer's own,
//! and cannot drift from it.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Fields, GenericParam, LitStr};

#[proc_macro_derive(Shaped, attributes(serde))]
pub fn derive_shaped(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match shaped(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// What a type's or a field's serde attributes say.
#[derive(Default)]
struct Serde {
    rename: Option<String>,
    rename_all: Option<String>,
    untagged: bool,
    tag: Option<String>,
    transparent: bool,
    flatten: bool,
    skip: bool,
    may_be_left_out: bool,
}

fn serde(attrs: &[Attribute]) -> syn::Result<Serde> {
    let mut found = Serde::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            let name = meta.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
            match name.as_str() {
                "rename" => found.rename = Some(meta.value()?.parse::<LitStr>()?.value()),
                "rename_all" => found.rename_all = Some(meta.value()?.parse::<LitStr>()?.value()),
                "tag" => found.tag = Some(meta.value()?.parse::<LitStr>()?.value()),
                "untagged" => found.untagged = true,
                "transparent" => found.transparent = true,
                "flatten" => found.flatten = true,
                "skip" | "skip_serializing" => found.skip = true,
                "skip_serializing_if" => {
                    meta.value()?.parse::<LitStr>()?;
                    found.may_be_left_out = true;
                }
                // Attributes that change nothing of the written shape.
                _ => {
                    if meta.input.peek(syn::Token![=]) {
                        meta.value()?.parse::<syn::Expr>()?;
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(found)
}

/// The doc comments, as one text.
fn about(attrs: &[Attribute]) -> String {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) => Some(s.value().trim().to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    lines.join(" ")
}

/// `name` as `rename_all` writes it.
fn renamed(name: &str, rule: Option<&str>) -> syn::Result<String> {
    let snake = || {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        }
        out
    };
    Ok(match rule {
        None => name.to_string(),
        Some("snake_case") => snake(),
        Some("lowercase") => name.to_lowercase(),
        Some("kebab-case") => snake().replace('_', "-"),
        Some(other) => return Err(syn::Error::new(proc_macro2::Span::call_site(), format!("Shaped does not know rename_all = {other:?}"))),
    })
}

/// The fields of a struct or a struct variant, as code pushing them on
/// `fields`.
fn fields_of(fields: &syn::FieldsNamed, rule: Option<&str>) -> syn::Result<Tokens> {
    let mut pushes = Vec::new();
    for field in &fields.named {
        let attrs = serde(&field.attrs)?;
        if attrs.skip {
            continue;
        }
        let ty = &field.ty;
        if attrs.flatten {
            pushes.push(quote! { ::eu4_harmony_remote::shape::flatten(&mut fields, <#ty as ::eu4_harmony_remote::shape::Shaped>::shape()); });
            continue;
        }
        let ident = field.ident.as_ref().expect("named").to_string();
        let name = match attrs.rename {
            Some(name) => name,
            None => renamed(&ident, rule)?,
        };
        let words = about(&field.attrs);
        let left_out = attrs.may_be_left_out;
        pushes.push(quote! {
            fields.push(::eu4_harmony_remote::shape::Field {
                name: #name,
                about: #words,
                shape: <#ty as ::eu4_harmony_remote::shape::Shaped>::shape(),
                may_be_left_out: #left_out,
            });
        });
    }
    Ok(quote! { #(#pushes)* })
}

fn shaped(input: &DeriveInput) -> syn::Result<Tokens> {
    let attrs = serde(&input.attrs)?;
    let ident = &input.ident;
    let class = ident.to_string();
    let words = about(&input.attrs);
    let body = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) if !attrs.transparent => {
                let pushes = fields_of(named, attrs.rename_all.as_deref())?;
                quote! {
                    let mut fields = ::std::vec::Vec::new();
                    #pushes
                    ::eu4_harmony_remote::shape::object(#class, #words, fields)
                }
            }
            Fields::Named(named) => {
                let ty = &named.named.first().expect("one field").ty;
                quote! { <#ty as ::eu4_harmony_remote::shape::Shaped>::shape() }
            }
            Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
                let ty = &unnamed.unnamed[0].ty;
                quote! { <#ty as ::eu4_harmony_remote::shape::Shaped>::shape() }
            }
            Fields::Unnamed(_) => return Err(syn::Error::new_spanned(ident, "Shaped knows tuple structs of one field only")),
            Fields::Unit => quote! { ::eu4_harmony_remote::shape::Shape::Nothing },
        },
        Data::Enum(data) => {
            let rule = attrs.rename_all.as_deref();
            let all_unit = data.variants.iter().all(|v| matches!(v.fields, Fields::Unit));
            if all_unit && !attrs.untagged {
                let mut names = Vec::new();
                for v in &data.variants {
                    let va = serde(&v.attrs)?;
                    if !va.skip {
                        names.push(match va.rename {
                            Some(n) => n,
                            None => renamed(&v.ident.to_string(), rule)?,
                        });
                    }
                }
                quote! { ::eu4_harmony_remote::shape::Shape::Words(::std::vec![#(#names),*]) }
            } else {
                let mut shapes = Vec::new();
                for v in &data.variants {
                    let va = serde(&v.attrs)?;
                    if va.skip {
                        continue;
                    }
                    let name = match va.rename {
                        Some(n) => n,
                        None => renamed(&v.ident.to_string(), rule)?,
                    };
                    let variant_class = format!("{class}{}", v.ident);
                    let inner = match &v.fields {
                        Fields::Unit if attrs.untagged => quote! { ::eu4_harmony_remote::shape::Shape::Nothing },
                        Fields::Unit => quote! { ::eu4_harmony_remote::shape::Shape::Words(::std::vec![#name]) },
                        Fields::Unnamed(u) if u.unnamed.len() == 1 => {
                            let ty = &u.unnamed[0].ty;
                            quote! { <#ty as ::eu4_harmony_remote::shape::Shaped>::shape() }
                        }
                        Fields::Unnamed(_) => return Err(syn::Error::new_spanned(&v.ident, "Shaped knows variants of one unnamed field only")),
                        Fields::Named(named) => {
                            let pushes = fields_of(named, None)?;
                            quote! {{
                                let mut fields = ::std::vec::Vec::new();
                                #pushes
                                ::eu4_harmony_remote::shape::object(#variant_class, "", fields)
                            }}
                        }
                    };
                    let shape = match (&attrs.tag, attrs.untagged, &v.fields) {
                        (_, true, _) => inner,
                        // Internally tagged: the tag a field of the object.
                        (Some(tag), _, _) => quote! {
                            ::eu4_harmony_remote::shape::tagged(#tag, #name, #variant_class, #inner)
                        },
                        (None, _, Fields::Unit) => inner,
                        // Externally tagged: { variant = inner }.
                        (None, _, _) => quote! {
                            ::eu4_harmony_remote::shape::Shape::Object {
                                class: #variant_class,
                                about: "",
                                fields: ::std::vec![::eu4_harmony_remote::shape::Field { name: #name, about: "", shape: #inner, may_be_left_out: false }],
                            }
                        },
                    };
                    shapes.push(shape);
                }
                quote! { ::eu4_harmony_remote::shape::Shape::Either(::std::vec![#(#shapes),*]) }
            }
        }
        Data::Union(_) => return Err(syn::Error::new_spanned(ident, "Shaped knows structs and enums")),
    };
    let mut generics = input.generics.clone();
    for param in &mut generics.params {
        if let GenericParam::Type(t) = param {
            t.bounds.push(syn::parse_quote!(::eu4_harmony_remote::shape::Shaped));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::eu4_harmony_remote::shape::Shaped for #ident #ty_generics #where_clause {
            fn shape() -> ::eu4_harmony_remote::shape::Shape {
                #body
            }
        }
    })
}
