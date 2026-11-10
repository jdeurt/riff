//! Numeric allocation limits apply before decimal expansion or exponentiation.
use std::str::FromStr;

use bigdecimal::BigDecimal;
use serde_json::Value;

use crate::{Error, Result};

/// Maximum nesting accepted for contract and JSON input values.
pub const MAX_INPUT_DEPTH: usize = 128;

/// Conservative bit-length bound for integers within coefficient and decimal-scale limits.
/// Reject larger host integers before allocating their decimal representation.
pub const MAX_INTEGER_BITS: u64 = 6804;

pub fn validate_numbers(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_INPUT_DEPTH {
        return Err(Error::Schema("JSON input nesting exceeds 128 levels.".to_owned()));
    }
    match value {
        Value::Number(number) => {
            let text = number.to_string();
            if text.len() > 2100 {
                return Err(Error::Schema("Numeric precision exceeds 1024 digits.".to_owned()));
            }
            let decimal =
                BigDecimal::from_str(&text).map_err(|error| Error::Schema(error.to_string()))?;
            if decimal.as_bigint_and_exponent().1.unsigned_abs() > 1024 {
                return Err(Error::Schema(
                    "Numeric decimal scale exceeds the supported limit 1024.".to_owned(),
                ));
            }
            let (coefficient, scale) = decimal.normalized().as_bigint_and_exponent();
            if scale.unsigned_abs() > 1024
                || coefficient.to_str_radix(10).trim_start_matches('-').len() > 1024
            {
                return Err(Error::Schema(
                    "Numeric precision or decimal scale exceeds the supported limit 1024."
                        .to_owned(),
                ));
            }
        }
        Value::Array(values) => {
            for child in values {
                validate_numbers(child, depth + 1)?;
            }
        }
        Value::Object(values) => {
            for child in values.values() {
                validate_numbers(child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
