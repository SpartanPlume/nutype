use std::collections::HashSet;

use proc_macro2::Span;

use crate::common::{
    models::{DeriveTrait, SpannedDeriveTrait},
    validate::validate_duplicates,
};

use super::models::{
    AnyDeriveTrait, AnyGuard, AnyRawGuard, AnySanitizer, AnyValidator, SpannedAnySanitizer,
    SpannedAnyValidator,
};

pub fn validate_any_guard(raw_guard: AnyRawGuard) -> Result<AnyGuard, syn::Error> {
    let AnyRawGuard {
        sanitizers,
        validators,
    } = raw_guard;

    let validators = validate_validators(validators)?;
    let sanitizers = validate_sanitizers(sanitizers)?;

    if validators.is_empty() {
        Ok(AnyGuard::WithoutValidation { sanitizers })
    } else {
        Ok(AnyGuard::WithValidation {
            sanitizers,
            validators,
        })
    }
}

fn validate_validators(
    validators: Vec<SpannedAnyValidator>,
) -> Result<Vec<AnyValidator>, syn::Error> {
    validate_duplicates(&validators, |kind| {
        format!("Duplicated validators `{kind}`.\nOh, maybe it's a time to take a break?")
    })?;

    let validators: Vec<AnyValidator> = validators.into_iter().map(|v| v.item).collect();
    Ok(validators)
}

fn validate_sanitizers(
    sanitizers: Vec<SpannedAnySanitizer>,
) -> Result<Vec<AnySanitizer>, syn::Error> {
    validate_duplicates(&sanitizers, |kind| {
        format!("Duplicated sanitizer `{kind}`.\nYou never know, what kind of error will be next!")
    })?;

    let sanitizers: Vec<_> = sanitizers.into_iter().map(|s| s.item).collect();
    Ok(sanitizers)
}

pub fn validate_any_derive_traits(
    guard: &AnyGuard,
    spanned_derive_traits: Vec<SpannedDeriveTrait>,
) -> Result<HashSet<AnyDeriveTrait>, syn::Error> {
    let mut traits = HashSet::with_capacity(24);
    let has_validation = guard.has_validation();

    for spanned_trait in spanned_derive_traits {
        let string_derive_trait =
            to_any_derive_trait(spanned_trait.item, has_validation, spanned_trait.span)?;
        traits.insert(string_derive_trait);
    }

    Ok(traits)
}

fn to_any_derive_trait(
    tr: DeriveTrait,
    _has_validation: bool,
    span: Span,
) -> Result<AnyDeriveTrait, syn::Error> {
    match tr {
        DeriveTrait::Debug => Ok(AnyDeriveTrait::Debug),
        DeriveTrait::Clone => Ok(AnyDeriveTrait::Clone),
        DeriveTrait::Copy => Ok(AnyDeriveTrait::Copy),
        DeriveTrait::PartialEq => Ok(AnyDeriveTrait::PartialEq),
        DeriveTrait::Eq => Ok(AnyDeriveTrait::Eq),
        DeriveTrait::Ord => Ok(AnyDeriveTrait::Ord),
        DeriveTrait::PartialOrd => Ok(AnyDeriveTrait::PartialOrd),
        DeriveTrait::Display => Ok(AnyDeriveTrait::Display),
        DeriveTrait::AsRef => Ok(AnyDeriveTrait::AsRef),
        DeriveTrait::Into => Ok(AnyDeriveTrait::Into),
        DeriveTrait::From => Ok(AnyDeriveTrait::From),
        DeriveTrait::Deref => Ok(AnyDeriveTrait::Deref),
        DeriveTrait::Borrow => Ok(AnyDeriveTrait::Borrow),
        DeriveTrait::FromStr => Ok(AnyDeriveTrait::FromStr),
        DeriveTrait::TryFrom => Ok(AnyDeriveTrait::TryFrom),
        DeriveTrait::Default => Ok(AnyDeriveTrait::Default),
        DeriveTrait::SerdeSerialize => Ok(AnyDeriveTrait::SerdeSerialize),
        DeriveTrait::SerdeDeserialize => Ok(AnyDeriveTrait::SerdeDeserialize),
        DeriveTrait::Hash => Ok(AnyDeriveTrait::Hash),
        DeriveTrait::ArbitraryArbitrary => Ok(AnyDeriveTrait::ArbitraryArbitrary),
        DeriveTrait::DieselNewType => Ok(AnyDeriveTrait::DieselNewType),
        DeriveTrait::SchemarsJsonSchema => {
            let msg =
                format!("Deriving of trait `{tr:?}` is not (yet) supported for an arbitrary type");
            Err(syn::Error::new(span, msg))
        }
    }
}
