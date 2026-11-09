//! Schema-violating mutations remain finite JSON and prefer local boundaries.
use bigdecimal::{BigDecimal, RoundingMode};
use serde_json::{Value, json};

use crate::node::SchemaNode;
use crate::{Error, Result};

pub fn violations(node: &SchemaNode) -> Result<Vec<Value>> {
    let data = &node.data;
    let mut candidates = Vec::new();
    for (bound, direction, ordinary_step) in [
        (data.minimum.as_ref(), -1_i32, 1_i32),
        (data.maximum.as_ref(), 1, 1),
        (data.exclusive_minimum.as_ref(), -1, 0),
        (data.exclusive_maximum.as_ref(), 1, 0),
    ] {
        if let Some(bound) = bound {
            let step = if bound.abs() > 9_007_199_254_740_991_u64 {
                let (coefficient, scale) = bound.abs().as_bigint_and_exponent();
                BigDecimal::new(coefficient, scale + 3)
                    .with_scale_round(0, RoundingMode::Ceiling)
                    .max(BigDecimal::from(1))
            } else {
                BigDecimal::from(ordinary_step)
            };
            let candidate = (bound + step * direction).normalized();
            candidates.push(serde_json::from_str(&candidate.to_plain_string())?);
        }
    }
    if data.min_length > 0 {
        candidates.push(json!(""));
    }
    if let Some(maximum) = data.max_length
        && maximum < 1024
    {
        candidates.push(json!("x".repeat(maximum + 1)));
    }
    if data.min_items > 0 {
        candidates.push(json!([]));
    }
    if let Some(maximum) = data.max_items
        && maximum < 1024
    {
        candidates.push(Value::Array(vec![Value::Null; maximum + 1]));
    }
    if data.min_properties > 0 || !data.required.is_empty() {
        candidates.push(json!({}));
    }
    let invalid: Vec<_> = candidates
        .into_iter()
        .filter(|value| crate::scalars::within_limits(value, 0) && !node.validator.is_valid(value))
        .collect();
    if !invalid.is_empty() {
        return Ok(invalid);
    }
    let fallback = [
        Value::Null,
        json!(false),
        json!(true),
        json!(0),
        json!(-1),
        json!(1),
        json!(""),
        json!("invalid"),
        json!([]),
        json!({}),
    ];
    let invalid: Vec<_> = fallback
        .into_iter()
        .filter(|value| crate::scalars::within_limits(value, 0) && !node.validator.is_valid(value))
        .collect();
    if invalid.is_empty() {
        return Err(Error::Generation(
            "No supported locally violating mutation exists.".to_owned(),
        ));
    }
    Ok(invalid)
}
