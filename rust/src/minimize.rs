//! Greedy reduction with caller-owned failure oracles.
use serde::Serialize;
use serde_json::{Value, json};

use crate::cases::equivalent;
use crate::node::Node;
use crate::{Error, Result};

/// A retained reproducer and its oracle budget usage.
#[derive(Clone, Debug, Serialize)]
pub struct MinimizationResult {
    /// Smallest reproducer found by this greedy traversal.
    pub value: Value,
    /// Number of oracle calls, including initial confirmation.
    pub evaluations: usize,
    /// Whether the oracle or candidate-work budget was consumed.
    pub budget_exhausted: bool,
}

pub fn minimize(
    node: &Node,
    input: &Value,
    maximum: usize,
    preserve: bool,
    oracle: &mut impl FnMut(&Value) -> Result<bool>,
) -> Result<MinimizationResult> {
    crate::limits::validate_numbers(input, 0)?;
    if maximum == 0 {
        return Err(Error::Argument("maxEvaluations must be positive.".to_owned()));
    }
    let mut current = input.clone();
    if preserve && !node.valid(&current) {
        return Err(Error::Argument(
            "Initial value violates the schema; disable schema preservation for malformed cases."
                .to_owned(),
        ));
    }
    if !oracle(&current)? {
        return Err(Error::Argument("Initial value does not reproduce the failure.".to_owned()));
    }
    let mut evaluations = 1;
    let mut remaining_candidates = maximum.saturating_mul(100);
    while evaluations < maximum {
        let accepted = accepted_reduction(
            node,
            &current,
            maximum,
            preserve,
            &mut evaluations,
            &mut remaining_candidates,
            oracle,
        )?;
        match accepted {
            Some(value) => current = value,
            None => {
                return Ok(MinimizationResult {
                    value: current,
                    evaluations,
                    budget_exhausted: evaluations >= maximum || remaining_candidates == 0,
                });
            }
        }
    }
    Ok(MinimizationResult { value: current, evaluations, budget_exhausted: true })
}

fn accepted_reduction(
    node: &Node,
    current: &Value,
    maximum: usize,
    preserve: bool,
    evaluations: &mut usize,
    remaining_candidates: &mut usize,
    oracle: &mut impl FnMut(&Value) -> Result<bool>,
) -> Result<Option<Value>> {
    for candidate in reductions(current) {
        let Some(remaining) = remaining_candidates.checked_sub(1) else {
            return Ok(None);
        };
        *remaining_candidates = remaining;
        if crate::limits::validate_numbers(&candidate, 0).is_err()
            || equivalent(current, &candidate)
            || (preserve && !node.valid(&candidate))
        {
            continue;
        }
        *evaluations += 1;
        if oracle(&candidate)? {
            return Ok(Some(candidate));
        }
        if *evaluations >= maximum {
            break;
        }
    }
    Ok(None)
}

fn reductions(value: &Value) -> Box<dyn Iterator<Item = Value> + '_> {
    match value {
        Value::Array(values) if !values.is_empty() => {
            Box::new(std::iter::once_with(|| json!([])).chain(values.iter().enumerate().flat_map(
                move |(index, child)| {
                    let removed = std::iter::once_with(move || {
                        let mut changed = values.clone();
                        let _ = changed.remove(index);
                        Value::Array(changed)
                    });
                    removed.chain(reductions(child).map(move |reduced| {
                        let mut changed = values.clone();
                        if let Some(value) = changed.get_mut(index) {
                            *value = reduced;
                        }
                        Value::Array(changed)
                    }))
                },
            )))
        }
        Value::String(text) if !text.is_empty() => {
            Box::new(std::iter::once_with(|| json!("")).chain((0..4).map(move |part| {
                let length = text.chars().count();
                let reduced: String = match part {
                    0 => text.chars().take(length / 2).collect(),
                    1 => text.chars().skip(length / 2).collect(),
                    2 => text.chars().take(length - 1).collect(),
                    _ => text.chars().skip(1).collect(),
                };
                Value::String(reduced)
            })))
        }
        Value::Object(values) if !values.is_empty() => {
            Box::new(std::iter::once_with(|| json!({})).chain(values.iter().flat_map(
                move |(name, child)| {
                    let removed = std::iter::once_with(move || {
                        let mut changed = values.clone();
                        let _ = changed.remove(name);
                        Value::Object(changed)
                    });
                    removed.chain(reductions(child).map(move |reduced| {
                        let mut changed = values.clone();
                        let _ = changed.insert(name.clone(), reduced);
                        Value::Object(changed)
                    }))
                },
            )))
        }
        _ => Box::new(scalar_reductions(value).into_iter()),
    }
}

fn scalar_reductions(value: &Value) -> Vec<Value> {
    let mut candidates = Vec::new();
    match value {
        Value::Bool(true) => candidates.push(Value::Bool(false)),
        Value::Number(number) => {
            if !equivalent(value, &json!(0)) {
                candidates.push(json!(0));
            }
            if let Some(value) = number.as_i64() {
                if value != 0 {
                    candidates.extend([
                        json!(value / 2),
                        json!(if value > 0 { value - 1 } else { value + 1 }),
                    ]);
                }
            } else if let Some(value) = number.as_f64()
                && value.is_finite()
                && value != 0.0
            {
                candidates.push(json!(value / 2.0));
                let toward_zero = if value > 0.0 { value - 1.0 } else { value + 1.0 };
                if toward_zero.abs() < value.abs() {
                    candidates.push(json!(toward_zero));
                }
            }
        }
        _ => {}
    }
    candidates
}
