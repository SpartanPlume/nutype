pub mod meta;

use std::{any::type_name, fmt::Debug, str::FromStr};

use proc_macro2::{Group, Ident, Span, TokenStream, TokenTree};
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    spanned::Spanned,
    token::Paren,
    Expr, LitInt, Token,
};

use crate::{
    common::models::{DeriveTrait, RawGuard, SpannedDeriveTrait},
    utils::match_feature,
};

use super::models::{Attributes, NewUnchecked};

/// ## Example
/// Input (token stream):
///     = 123
/// Output (parsed value):
///    123
pub fn parse_value_as_number<T, ITER>(mut iter: ITER) -> Result<(T, ITER), syn::Error>
where
    T: FromStr,
    <T as FromStr>::Err: Debug,
    ITER: Iterator<Item = TokenTree>,
{
    let token_eq = iter.next().expect("Expected token `=`");
    assert_eq!(token_eq.to_string(), "=", "Expected token `=`");

    let (num_str, span) = read_number(&mut iter)?;

    let value: T = sanitize_number(&num_str).parse::<T>().map_err(|_err| {
        let msg = format!("Expected {}, got `{}`", type_name::<T>(), num_str);
        syn::Error::new(span, msg)
    })?;

    Ok((value, iter))
}

fn read_number<ITER>(iter: &mut ITER) -> Result<(String, Span), syn::Error>
where
    ITER: Iterator<Item = TokenTree>,
{
    let mut output = String::with_capacity(16);
    let mut token_value = iter.next().expect("Expected number");
    let span = token_value.span();
    let mut t = token_value.to_string();

    // If it starts with `-` (negative number), add it to output and parse the next token.
    if t == "-" {
        output.push_str(&t);
        token_value = iter.next().expect("Expected number");
        t = token_value.to_string();
    }

    output.push_str(&t);
    Ok((output, span))
}

fn sanitize_number(val: &str) -> String {
    val.replace('_', "")
}

pub fn try_unwrap_ident(token: TokenTree) -> Result<Ident, syn::Error> {
    match token {
        TokenTree::Ident(ident) => Ok(ident),
        _ => {
            let error = syn::Error::new(token.span(), "#[nutype] expected ident");
            Err(error)
        }
    }
}

pub fn try_unwrap_group(token: TokenTree) -> Result<Group, syn::Error> {
    match token {
        TokenTree::Group(group) => Ok(group),
        _ => {
            let error = syn::Error::new(token.span(), "#[nutype] expected group");
            Err(error)
        }
    }
}

pub fn parse_nutype_attributes<S, V>(
    parse_sanitize_attrs: impl Fn(TokenStream) -> Result<Vec<S>, syn::Error>,
    parse_validate_attrs: impl Fn(TokenStream) -> Result<Vec<V>, syn::Error>,
) -> impl FnOnce(TokenStream) -> Result<Attributes<RawGuard<S, V>>, syn::Error> {
    move |input: TokenStream| {
        let mut raw_guard = RawGuard {
            sanitizers: vec![],
            validators: vec![],
        };

        // The variable `new_unchecked` is needed to be mutable for the case when `new_unchecked`
        // feature flag is enabled.
        #[allow(unused_mut)]
        let mut new_unchecked = NewUnchecked::Off;

        // Value which is used to derive Default trait
        let mut maybe_default_value: Option<TokenStream> = None;

        let mut iter = input.into_iter();

        loop {
            let token = match iter.next() {
                Some(t) => t,
                None => {
                    let attributes = Attributes {
                        guard: raw_guard,
                        new_unchecked,
                        maybe_default_value,
                    };
                    return Ok(attributes);
                }
            };

            let ident = try_unwrap_ident(token)?;

            match ident.to_string().as_ref() {
                "sanitize" => {
                    let token = iter.next().ok_or_else(|| {
                        let msg = "`sanitize` must be used with parenthesis.\nFor example:\n\n    sanitize(trim)\n\n";
                        syn::Error::new(ident.span(), msg)
                    })?;
                    let group = try_unwrap_group(token)?;
                    let sanitize_stream = group.stream();
                    raw_guard.sanitizers = parse_sanitize_attrs(sanitize_stream)?;
                }
                "validate" => {
                    let token = iter.next().ok_or_else(|| {
                        let msg = "`validate` must be used with parenthesis.\nFor example:\n\n    validate(max = 99)\n\n";
                        syn::Error::new(ident.span(), msg)
                    })?;
                    let group = try_unwrap_group(token)?;
                    let validate_stream = group.stream();
                    raw_guard.validators = parse_validate_attrs(validate_stream)?;
                }
                "default" => {
                    {
                        // Take `=` sign
                        if let Some(eq_t) = iter.next() {
                            if !is_eq(&eq_t) {
                                return Err(syn::Error::new(
                                    ident.span(),
                                    "Invalid syntax for `default`. Expected `=`, got `{eq_t}`",
                                ));
                            }
                        } else {
                            return Err(syn::Error::new(
                                ident.span(),
                                "Invalid syntax for `default`. Missing `=`",
                            ));
                        }
                    }
                    // TODO: parse it properly till some delimeter?
                    let default_value = iter
                        .next()
                        .ok_or_else(|| syn::Error::new(ident.span(), "Missing default value"))?;
                    maybe_default_value = Some(TokenStream::from(default_value));
                }
                "new_unchecked" => {
                    match_feature!("new_unchecked",
                        // The feature is not enabled, so we return an error
                        on => {
                            // Try to look forward and return a good helpful error if `new_unchecked` is
                            // used incorrectly.
                            // Correct:
                            //    new_unchecked
                            // Incorrect:
                            //    new_unchecked()
                            //    new_unchecked(foo = 13)
                            let maybe_next_token = iter.clone().next();
                            match maybe_next_token.map(try_unwrap_ident) {
                                None | Some(Ok(_)) => {
                                    new_unchecked = NewUnchecked::On;
                                }
                                Some(Err(err)) => {
                                    let msg = "new_unchecked does not support any options";
                                    return Err(syn::Error::new(err.span(), msg));
                                }
                            }
                        },

                        // The feature `new_unchecked` is enabled
                        off => {
                            let msg = "To generate ::new_unchecked() function, the feature `new_unchecked` of crate `nutype` needs to be enabled.\nBut you ought to know: generally, THIS IS A BAD IDEA.\nUse it only when you really need it.";
                            return Err(syn::Error::new(ident.span(), msg));
                        }
                    )
                }
                unknown => {
                    let msg = format!("Unknown #[nutype] option: `{unknown}`");
                    let error = syn::Error::new(ident.span(), msg);
                    return Err(error);
                }
            }
        }
    }
}

pub fn split_and_parse<SEP, PRS, OUT>(
    tokens: Vec<TokenTree>,
    is_separator: SEP,
    parse: PRS,
) -> Result<Vec<OUT>, syn::Error>
where
    SEP: Fn(&TokenTree) -> bool,
    PRS: Fn(Vec<TokenTree>) -> Result<OUT, syn::Error>,
{
    tokens
        .split(is_separator)
        .filter(|subtokens| !subtokens.is_empty())
        .map(|subtokens| parse(subtokens.to_owned()))
        .collect()
}

pub fn is_comma(token: &TokenTree) -> bool {
    match token {
        TokenTree::Punct(punct) => punct.as_char() == ',',
        _ => false,
    }
}

pub fn is_eq(token: &TokenTree) -> bool {
    match token {
        TokenTree::Punct(punct) => punct.as_char() == '=',
        _ => false,
    }
}

// Context:
//   with = |s: String| s.uppercase()
// Input:
//   = |s: String| s.to_uppercase()
// Output
//   |s: String| s.to_uppercase()
pub fn parse_with_token_stream<'a>(
    mut token_iter: impl Iterator<Item = &'a TokenTree>,
    with_span: Span,
) -> Result<TokenStream, syn::Error> {
    {
        // Take `=` sign
        if let Some(eq_t) = token_iter.next() {
            if !is_eq(eq_t) {
                let span = with_span;
                return Err(syn::Error::new(
                    span,
                    "Invalid syntax for `with`. Expected `=`, got `{eq_t}`",
                ));
            }
        } else {
            return Err(syn::Error::new(
                with_span,
                "Invalid syntax for `with`. Missing `=`",
            ));
        }
    }

    // Return the rest as TokenStream
    let rest = TokenStream::from_iter(token_iter.cloned());
    Ok(rest)
}

pub fn is_doc_attribute(attribute: &syn::Attribute) -> bool {
    match attribute.path().segments.first() {
        Some(path_segment) => path_segment.ident == "doc",
        None => false,
    }
}

pub fn is_derive_attribute(attribute: &syn::Attribute) -> bool {
    match attribute.path().segments.first() {
        Some(path_segment) => path_segment.ident == "derive",
        None => false,
    }
}

pub fn parse_derive_traits(
    attributes: &[syn::Attribute],
) -> Result<Vec<SpannedDeriveTrait>, syn::Error> {
    let traits: Vec<Vec<SpannedDeriveTrait>> = attributes
        .iter()
        .filter(|a| is_derive_attribute(a))
        .map(parse_derive_attr)
        .collect::<Result<_, syn::Error>>()?;
    Ok(traits.into_iter().flatten().collect())
}

fn parse_derive_attr(attr: &syn::Attribute) -> Result<Vec<SpannedDeriveTrait>, syn::Error> {
    let meta = &attr.meta;
    match meta {
        syn::Meta::Path(path) => {
            let msg = format!("Expected #[derive(...)], got: {path:?}");
            Err(syn::Error::new(Span::call_site(), msg))
        }
        syn::Meta::NameValue(name_value) => {
            let msg = format!("Expected #[derive(...)], got: {name_value:?}");
            Err(syn::Error::new(Span::call_site(), msg))
        }
        syn::Meta::List(list) => {
            let derive_traits: Vec<SpannedDeriveTrait> = list
                .tokens
                .clone()
                .into_iter()
                .map(parse_token_into_derive_trait)
                .collect::<Result<Vec<Option<SpannedDeriveTrait>>, syn::Error>>()?
                .into_iter()
                .flatten()
                .collect();
            Ok(derive_traits)
        }
    }
}

fn parse_token_into_derive_trait(
    token: TokenTree,
) -> Result<Option<SpannedDeriveTrait>, syn::Error> {
    match token {
        TokenTree::Ident(ident) => {
            let derive_trait = parse_ident_into_derive_trait(ident)?;
            Ok(Some(derive_trait))
        }
        TokenTree::Punct(ref punct) => match punct.as_char() {
            ',' => Ok(None),
            '*' => Err(syn::Error::new(
                token.span(),
                "Asterisk derive is not longer supported",
            )),
            _ => Err(syn::Error::new(
                token.span(),
                format!("Unexpected `{token}`"),
            )),
        },
        _ => Err(syn::Error::new(
            token.span(),
            format!("Unexpected `{token}`"),
        )),
    }
}

fn parse_ident_into_derive_trait(ident: Ident) -> Result<SpannedDeriveTrait, syn::Error> {
    let derive_trait = match ident.to_string().as_ref() {
        "Debug" => DeriveTrait::Debug,
        "Display" => DeriveTrait::Display,
        "Clone" => DeriveTrait::Clone,
        "Copy" => DeriveTrait::Copy,
        "PartialEq" => DeriveTrait::PartialEq,
        "Eq" => DeriveTrait::Eq,
        "PartialOrd" => DeriveTrait::PartialOrd,
        "Ord" => DeriveTrait::Ord,
        "FromStr" => DeriveTrait::FromStr,
        "AsRef" => DeriveTrait::AsRef,
        "Deref" => DeriveTrait::Deref,
        "TryFrom" => DeriveTrait::TryFrom,
        "From" => DeriveTrait::From,
        "Into" => DeriveTrait::Into,
        "Hash" => DeriveTrait::Hash,
        "Borrow" => DeriveTrait::Borrow,
        "Default" => DeriveTrait::Default,
        "Serialize" => {
            match_feature!("serde",
                on => DeriveTrait::SerdeSerialize,
                off => {
                    return Err(syn::Error::new(ident.span(), "To derive Serialize, the feature `serde` of the crate `nutype` needs to be enabled."));
                },
            )
        }
        "Deserialize" => {
            match_feature!("serde",
                on => DeriveTrait::SerdeDeserialize,
                off => {
                    return Err(syn::Error::new(ident.span(), "To derive Deserialize, the feature `serde` of the crate `nutype` needs to be enabled."));
                },
            )
        }
        "JsonSchema" => {
            match_feature!("schemars08",
                on => DeriveTrait::SchemarsJsonSchema,
                off => {
                    return Err(syn::Error::new(ident.span(), "To derive JsonSchema, the feature `schemars08` of the crate `nutype` needs to be enabled."));
                }
            )
        }
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                format!("unsupported trait derive: {ident}"),
            ));
        }
    };
    let spanned_trait = SpannedDeriveTrait {
        item: derive_trait,
        span: ident.span(),
    };
    Ok(spanned_trait)
}

#[derive(Debug)]
pub struct ParseableAttributes<Sanitizer, Validator> {
    pub sanitizers: Vec<Sanitizer>,
    pub validators: Vec<Validator>,
    pub new_unchecked: NewUnchecked,
    pub default: Option<Expr>,
}

// By some reason Default cannot be derive.
impl<Sanitizer, Validator> Default for ParseableAttributes<Sanitizer, Validator> {
    fn default() -> Self {
        Self {
            sanitizers: vec![],
            validators: vec![],
            new_unchecked: NewUnchecked::Off,
            default: None,
        }
    }
}

impl<Sanitizer: Parse, Validator: Parse> Parse for ParseableAttributes<Sanitizer, Validator> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attrs = ParseableAttributes::default();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            if ident == "sanitize" {
                if input.peek(Paren) {
                    let content;
                    parenthesized!(content in input);
                    let items = content.parse_terminated(Sanitizer::parse, Token![,])?;
                    attrs.sanitizers = items.into_iter().collect();
                } else {
                    let msg = concat!(
                        "`sanitize` must be used with parenthesis.\n",
                        "For example:\n\n",
                        "    sanitize(trim)\n\n"
                    );
                    return Err(syn::Error::new(ident.span(), msg));
                }
            } else if ident == "validate" {
                if input.peek(Paren) {
                    let content;
                    parenthesized!(content in input);
                    let items = content.parse_terminated(Validator::parse, Token![,])?;
                    attrs.validators = items.into_iter().collect();
                } else {
                    let msg = concat!(
                        "`validate` must be used with parenthesis.\n",
                        "For example:\n\n",
                        "    validate(max = 99)\n\n"
                    );
                    return Err(syn::Error::new(ident.span(), msg));
                }
            } else if ident == "default" {
                let _eq: Token![=] = input.parse()?;
                let default_expr: Expr = input.parse()?;
                attrs.default = Some(default_expr);
            } else if ident == "new_unchecked" {
                match_feature!("new_unchecked",
                    // The feature is not enabled, so we return an error
                    on => {
                        attrs.new_unchecked = NewUnchecked::On;
                    },
                    off => {
                        let msg = concat!(
                            "To generate ::new_unchecked() function, the feature `new_unchecked` of crate `nutype` needs to be enabled.\n",
                            "But you ought to know: generally, THIS IS A BAD IDEA.\nUse it only when you really need it."
                        );
                        return Err(syn::Error::new(ident.span(), msg));
                    }
                )
            } else {
                let msg = format!("Unknown attribute `{ident}`");
                return Err(syn::Error::new(ident.span(), msg));
            }

            // Parse `,` unless it's the end of the stream
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        Ok(attrs)
    }
}

pub fn parse_integer<T: FromStr>(input: ParseStream) -> syn::Result<(T, Span)> {
    parse_number::<T, LitInt>(input)
}

fn parse_number<T, Literal>(input: ParseStream) -> syn::Result<(T, Span)>
where
    T: FromStr,
    Literal: Parse + ToString + Spanned,
{
    let mut number_str = String::with_capacity(16);
    if input.peek(Token![-]) {
        let _: Token![-] = input.parse()?;
        number_str.push('-');
    }

    let lit: Literal = input.parse()?;
    number_str.push_str(&lit.to_string().replace('_', ""));

    let number: T = number_str.parse::<T>().map_err(|_err| {
        let msg = format!("Expected {}, got `{}`", type_name::<T>(), number_str);
        syn::Error::new(lit.span(), msg)
    })?;

    Ok((number, lit.span()))
}
