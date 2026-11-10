//! Bounded numeric, string, and regex generation.
use std::str::FromStr;

use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use rand::{Rng, RngCore};
use serde_json::Value;

use crate::node::SchemaNode;
use crate::{Error, Result};

pub const MAX_SIZE: usize = 1024;
pub const EDGES: &[&str] =
    &["", " ", "\t\n", "é", "e\u{301}", "日本語", "🦊", "\0", "'\"\\", "<>&"];
const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 é日本🦊\t\n";

pub fn size(
    minimum: usize,
    maximum: Option<usize>,
    default: usize,
    boundary: bool,
    random: &mut impl RngCore,
) -> Result<usize> {
    let upper = maximum.unwrap_or_else(|| minimum.max(default)).min(MAX_SIZE);
    if minimum > upper {
        return Err(Error::Generation(
            "Inconsistent size bounds or minimum exceeds safety limit 1024.".to_owned(),
        ));
    }
    Ok(if boundary || random.random_range(0..3) == 0 {
        if random.random::<bool>() { minimum } else { upper }
    } else {
        random.random_range(minimum..=upper)
    })
}

pub fn choose<'a, T>(values: &'a [T], random: &mut impl RngCore) -> Result<&'a T> {
    if values.is_empty() {
        return Err(Error::Generation("Cannot choose from an empty source.".to_owned()));
    }
    values
        .get(random.random_range(0..values.len()))
        .ok_or_else(|| Error::Generation("Candidate selection failed.".to_owned()))
}

pub fn regex_value(pattern: &str, random: &mut impl RngCore) -> Result<String> {
    if pattern.len() > MAX_SIZE {
        return Err(Error::Generation("Pattern exceeds its source safety limit.".to_owned()));
    }
    let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
    let pattern = if pattern.ends_with("\\$") {
        pattern
    } else {
        pattern.strip_suffix('$').unwrap_or(pattern)
    };
    let translated = jsonschema_regex::to_rust_regex(pattern)
        .map_err(|()| Error::Generation("Unsupported ECMAScript pattern generation.".to_owned()))?;
    let compiled = rand_regex::Regex::compile(&translated, 32).map_err(|error| {
        Error::Generation(format!(
            "Unsupported regex generation; use examples or a provider: {error}"
        ))
    })?;
    if compiled.capacity() > MAX_SIZE * 4 {
        return Err(Error::Generation("Pattern output exceeds its allocation limit.".to_owned()));
    }
    let value: String = random.sample(compiled);
    if value.chars().count() > MAX_SIZE {
        return Err(Error::Generation("Pattern output exceeds string safety limit.".to_owned()));
    }
    Ok(value)
}

pub fn string(
    node: &SchemaNode,
    boundary: bool,
    realistic: bool,
    random: &mut impl RngCore,
) -> Result<String> {
    if let Some(pattern) = &node.data.pattern {
        return regex_value(pattern, random);
    }
    if !realistic && random.random::<bool>() {
        return Ok((*choose(EDGES, random)?).to_owned());
    }
    let length = size(node.data.min_length, node.data.max_length, 32, boundary, random)?;
    let alphabet: Vec<_> = ALPHABET.chars().collect();
    (0..length).map(|_| choose(&alphabet, random).copied()).collect()
}

fn rational(decimal: &BigDecimal) -> Result<BigRational> {
    let (coefficient, scale) = decimal.as_bigint_and_exponent();
    let exponent = u32::try_from(scale.unsigned_abs())
        .map_err(|error| Error::Generation(error.to_string()))?;
    let power = BigInt::from(10).pow(exponent);
    Ok(if scale < 0 {
        BigRational::from_integer(coefficient * power)
    } else {
        BigRational::new(coefficient, power)
    })
}

fn numeric_step(
    node: &SchemaNode,
    minimum: &BigDecimal,
    maximum: &BigDecimal,
    integer: bool,
) -> Result<BigDecimal> {
    let step = if let Some(multiple) = &node.data.multiple_of {
        multiple.clone()
    } else if integer {
        BigDecimal::from(1)
    } else {
        let (coefficient, scale) = (maximum - minimum).abs().as_bigint_and_exponent();
        BigDecimal::from_str("0.01")
            .map_err(|error| Error::Generation(error.to_string()))?
            .min(BigDecimal::new(coefficient, scale + 2))
    };
    if integer { Ok(BigDecimal::from(rational(&step)?.numer().clone())) } else { Ok(step) }
}

pub fn number(
    node: &SchemaNode,
    integer: bool,
    boundary: bool,
    random: &mut impl RngCore,
) -> Result<Value> {
    let data = &node.data;
    let mut minimum = data
        .minimum
        .as_ref()
        .or(data.exclusive_minimum.as_ref())
        .cloned()
        .unwrap_or_else(|| BigDecimal::from(-1000));
    let mut maximum = data
        .maximum
        .as_ref()
        .or(data.exclusive_maximum.as_ref())
        .cloned()
        .unwrap_or_else(|| BigDecimal::from(1000));
    if data.minimum.is_none() && data.exclusive_minimum.is_none() {
        minimum = minimum.min(&maximum - 1000);
    }
    if data.maximum.is_none() && data.exclusive_maximum.is_none() {
        maximum = maximum.max(&minimum + 1000);
    }
    if minimum > maximum {
        return Err(Error::Generation("Numeric bounds have no supported value.".to_owned()));
    }
    if !integer && data.multiple_of.is_none() && minimum == maximum {
        return serde_json::from_str(&minimum.to_plain_string()).map_err(Error::from);
    }
    let step = numeric_step(node, &minimum, &maximum, integer)?;
    if step <= 0 {
        return Err(Error::Generation("No supported numeric step.".to_owned()));
    }
    let rational_step = rational(&step)?;
    let lower = rational(&minimum)? / &rational_step;
    let upper = rational(&maximum)? / &rational_step;
    let mut first = lower.numer().div_ceil(lower.denom());
    let mut last = upper.numer().div_floor(upper.denom());
    if let Some(exclusive) = &data.exclusive_minimum {
        let quotient = rational(exclusive)? / &rational_step;
        first = first.max(quotient.numer().div_floor(quotient.denom()) + BigInt::from(1));
    }
    if let Some(exclusive) = &data.exclusive_maximum {
        let quotient = rational(exclusive)? / &rational_step;
        last = last.min(quotient.numer().div_ceil(quotient.denom()) - BigInt::from(1));
    }
    if first > last {
        return Err(Error::Generation("Numeric bounds contain no supported multiple.".to_owned()));
    }
    let zero = BigInt::from(0).max(first.clone()).min(last.clone());
    let index = if boundary || random.random_range(0..3) == 0 {
        choose(&[first, last, zero], random)?.clone()
    } else {
        let range = &last - &first + BigInt::from(1);
        let numerator = range * BigInt::from(random.next_u64());
        let denominator = BigInt::from(u64::MAX) + BigInt::from(1);
        &first + numerator.div_floor(&denominator)
    };
    let value = (step * BigDecimal::from(index)).normalized();
    serde_json::from_str(&value.to_plain_string()).map_err(Error::from)
}

pub fn within_limits(value: &Value, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match value {
        Value::String(text) => text.chars().count() <= MAX_SIZE,
        Value::Array(values) => {
            values.len() <= MAX_SIZE && values.iter().all(|child| within_limits(child, depth + 1))
        }
        Value::Object(values) => {
            values.len() <= MAX_SIZE && values.values().all(|child| within_limits(child, depth + 1))
        }
        Value::Number(_) => crate::limits::validate_numbers(value, depth).is_ok(),
        _ => true,
    }
}
