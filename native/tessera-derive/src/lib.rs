use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{Data, DeriveInput, LitStr, parse_macro_input, parse_quote};

#[proc_macro_derive(EnumCount)]
pub fn derive_enum_count(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_name = &input.ident;

    let count = match &input.data {
        Data::Enum(data) => data.variants.len(),

        _ => {
            return syn::Error::new_spanned(&input, "EnumCount can only be derived for enums")
                .to_compile_error()
                .into();
        }
    };

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    quote! {
        #[automatically_derived]
        impl #impl_generics #enum_name #type_generics #where_clause {
            pub const COUNT: usize = #count;
        }
    }
    .into()
}

#[proc_macro_derive(AllArray)]
pub fn derive_all_variants_array(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_name = &input.ident;

    let variants = match &input.data {
        Data::Enum(data) => data.variants.iter().map(|v| &v.ident).collect::<Vec<_>>(),

        _ => {
            return syn::Error::new_spanned(&input, "AllVariantsArray can only be derived for enums")
                .to_compile_error()
                .into();
        }
    };

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let count = variants.len();

    quote! {
        #[automatically_derived]
        impl #impl_generics #enum_name #type_generics #where_clause {
            pub const ALL: [#enum_name; #count] = [
                #( #enum_name::#variants, )*
            ];
        }
    }
    .into()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 1);
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if !c.is_uppercase() {
            result.push(c);
            continue;
        }

        if i > 0 && !result.ends_with('_') {
            let prev_lower = chars.get(i - 1).is_some_and(|&c| c.is_lowercase());
            let next_lower = chars.get(i + 1).is_some_and(|&c| c.is_lowercase());
            if prev_lower || next_lower {
                result.push('_');
            }
        }
        result.extend(c.to_lowercase());
    }

    result
}

#[proc_macro_attribute]
pub fn mc_registry_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as DeriveInput);

    let mut untagged = false;
    let mut tag = None;
    let mut namespace = None;

    let parser = syn::meta::parser(|meta| {
        let ident = meta
            .path
            .get_ident()
            .ok_or_else(|| meta.error("Unknown attribute, expected identifier"))?;

        if ident == "tag" {
            let literal = meta.value()?.parse::<LitStr>()?.value();
            tag = Some(literal);
        } else if ident == "namespace" {
            let literal = meta.value()?.parse::<LitStr>()?.value();
            namespace = Some(literal);
        } else if ident == "untagged" {
            untagged = true;
        } else {
            return Err(meta.error("Unknown attribute, expected \"tag\", \"namespace\" or \"untagged\""));
        }
        Ok(())
    });

    if let Err(err) = parser.parse(attr) {
        return err.to_compile_error().into();
    }

    if untagged && tag.is_some() {
        return syn::Error::new_spanned(&input, "\"#[mc_registry(untagged)]\" cannot be combined with \"tag\"")
            .to_compile_error()
            .into();
    }

    let namespace = namespace.unwrap_or_else(|| "minecraft".to_string());

    let variants = match &mut input.data {
        Data::Enum(data) => &mut data.variants,
        _ => {
            return syn::Error::new_spanned(&input, "mc_registry_enum can only be used on enums")
                .to_compile_error()
                .into();
        }
    };

    if !untagged {
        let tag = tag.unwrap_or_else(|| "type".to_string());
        input.attrs.push(parse_quote!(#[serde(tag = #tag)]));
    }

    for variant in variants {
        let mut rename = None;
        let mut id = None;
        let mut is_other = false;
        let mut variant_err = None;

        variant.attrs.retain(|attr| {
            if !attr.path().is_ident("mc_registry") {
                return true;
            }
            let res = attr.parse_nested_meta(|meta| {
                let ident = meta
                    .path
                    .get_ident()
                    .ok_or_else(|| meta.error("Unknown attribute, expected identifier"))?;
                if ident == "rename" {
                    let literal = meta.value()?.parse::<LitStr>()?.value();
                    if literal.contains(':') {
                        return Err(meta.error("Invalid \"rename\" format, expected \"<path>\""));
                    }
                    rename = Some(literal);
                } else if ident == "id" {
                    let literal = meta.value()?.parse::<LitStr>()?.value();
                    if !literal
                        .split_once(':')
                        .is_some_and(|(namespace, path)| !namespace.is_empty() && !path.is_empty())
                    {
                        return Err(meta.error("Invalid \"id\" format, expected \"<namespace>:<path>\""));
                    }
                    id = Some(literal);
                } else if ident == "other" {
                    is_other = true;
                } else {
                    return Err(meta.error("Unknown attribute, expected \"rename\", \"id\" or \"other\""));
                }
                Ok(())
            });

            if let Err(err) = res {
                variant_err = Some(err);
            }

            false
        });

        if let Some(e) = variant_err {
            return e.to_compile_error().into();
        }

        if is_other && (id.is_some() || rename.is_some()) {
            return syn::Error::new_spanned(
                &variant,
                "\"#[mc_registry(other)]\" cannot be combined with \"id\" or \"rename\"",
            )
            .to_compile_error()
            .into();
        }

        if id.is_some() && rename.is_some() {
            return syn::Error::new_spanned(
                &variant,
                "\"#[mc_registry(id = \"...\")]\" cannot be combined with \"#[mc_registry(rename = \"...\")]\"",
            )
            .to_compile_error()
            .into();
        }

        if is_other {
            variant.attrs.push(parse_quote!(#[serde(other)]));
            continue;
        }

        let path = rename.unwrap_or_else(|| to_snake_case(&variant.ident.to_string()));
        let id = id.unwrap_or_else(|| format!("{}:{}", namespace, path));
        let alias = id.split_once(':').map_or(path.as_str(), |(_, path)| path);

        variant.attrs.push(parse_quote!(#[serde(rename = #id, alias = #alias)]));
    }

    TokenStream::from(quote!(#input))
}
