//! Local JSON/YAML contracts with exact numeric lexemes and bounded parsing.
use std::borrow::Cow;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use bigdecimal::BigDecimal;
use saphyr::{Scalar, ScalarStyle, Yaml, YamlLoader};
use saphyr_parser::{Event, Parser, SpannedEventReceiver};
use serde::Deserialize;
use serde_json::{Map, Number, Value};

use crate::{Error, Result};

/// Numeric representations accepted by a consumer's JSON interface.
#[derive(Clone, Copy, Debug)]
pub enum NumericMode {
    /// Preserve arbitrary JSON numbers within core allocation limits.
    Exact,
    /// Preserve arbitrary integers and decimal values that round-trip through binary64.
    NativeFloat,
    /// Require binary64 round-tripping and the interoperable safe-integer range.
    SafeFloat,
}

impl NumericMode {
    /// Check that JSON data can cross this numeric interface without alteration.
    ///
    /// # Errors
    /// Rejects excessive precision, nesting, nonfinite projections, and rounding.
    pub fn validate(self, value: &Value) -> Result<()> {
        crate::limits::validate_numbers(value, 0)?;
        self.check(value)
    }

    fn check(self, value: &Value) -> Result<()> {
        match value {
            Value::Number(number) if !matches!(self, Self::Exact) => {
                let original = BigDecimal::from_str(&number.to_string())
                    .map_err(|error| Error::Schema(error.to_string()))?;
                let integer = original.normalized().as_bigint_and_exponent().1 <= 0;
                if integer
                    && !number.to_string().contains(['.', 'e', 'E'])
                    && matches!(self, Self::NativeFloat)
                {
                    return Ok(());
                }
                if integer
                    && matches!(self, Self::SafeFloat)
                    && original.abs() > 9_007_199_254_740_991_u64
                {
                    return Err(Error::Schema(
                        "JSON number exceeds the safe-integer range.".to_owned(),
                    ));
                }
                let projected = number.as_f64().and_then(Number::from_f64).ok_or_else(|| {
                    Error::Schema("JSON number has no finite binary64 representation.".to_owned())
                })?;
                let projected = BigDecimal::from_str(&projected.to_string())
                    .map_err(|error| Error::Schema(error.to_string()))?;
                if original != projected {
                    return Err(Error::Schema(
                        "JSON number cannot preserve its decimal value.".to_owned(),
                    ));
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.check(child)?;
                }
            }
            Value::Object(values) => {
                for child in values.values() {
                    self.check(child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Inline JSON or a local contract filename, never a network location.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase", deny_unknown_fields)]
pub enum Document {
    /// An already supplied JSON value.
    Inline(Value),
    /// A local UTF-8 JSON/YAML file.
    File(std::path::PathBuf),
}

impl Document {
    /// Load a document without resolving remote references or executing YAML tags.
    ///
    /// # Errors
    /// Rejects I/O failures, invalid JSON/YAML, aliases, tags, duplicate keys, and limits.
    pub fn load(&self) -> Result<Value> {
        match self {
            Self::Inline(value) => Ok(value.clone()),
            Self::File(path) => read_document(path),
        }
    }
}

/// Read one local contract, limiting source input to 16 MiB.
///
/// # Errors
/// Returns a schema error for I/O, syntax, numeric allocation, or YAML policy failures.
pub fn read_document(path: impl AsRef<Path>) -> Result<Value> {
    const MAX_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;
    let path = path.as_ref();
    let load = || -> Result<Value> {
        let file = File::open(path).map_err(|error| Error::Schema(error.to_string()))?;
        let mut source = String::new();
        let _ = file
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_string(&mut source)
            .map_err(|error| Error::Schema(error.to_string()))?;
        if !u64::try_from(source.len()).is_ok_and(|size| size <= MAX_DOCUMENT_BYTES) {
            return Err(Error::Schema("Contract source exceeds 16 MiB.".to_owned()));
        }
        if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")) {
            let _: Value = serde_json::from_str(&source)?;
        }
        parse_document(&source)
    };
    load().map_err(|error| {
        Error::Schema(format!("Unable to read contract {}: {error}", path.display()))
    })
}

/// Parse one YAML 1.2 document, including JSON, without rounding numbers.
///
/// # Errors
/// Rejects multiple documents, aliases, custom tags, non-string keys, and duplicates.
pub fn parse_document(source: impl AsRef<str>) -> Result<Value> {
    let source = source.as_ref();
    let parser = Parser::new_from_str(source);
    let mut loader = YamlLoader::<Yaml<'_>>::default();
    loader.early_parse(false);
    let mut depth = 0_usize;
    for item in parser {
        let (mut event, span) = item.map_err(|error| Error::Schema(error.to_string()))?;
        let opens_collection = matches!(event, Event::SequenceStart(..) | Event::MappingStart(..));
        let closes_collection = matches!(event, Event::SequenceEnd | Event::MappingEnd);
        let scalar = matches!(event, Event::Scalar(..));
        if (opens_collection || scalar) && depth > crate::limits::MAX_INPUT_DEPTH {
            return Err(Error::Schema("YAML input nesting exceeds 128 levels.".to_owned()));
        }
        if matches!(
            &event,
            Event::Scalar(_, _, _, Some(_))
                | Event::SequenceStart(_, Some(_))
                | Event::MappingStart(_, Some(_))
        ) {
            return Err(Error::Schema(
                "YAML tags are unsupported; use plain JSON-compatible data.".to_owned(),
            ));
        }
        if matches!(event, Event::Alias(_)) {
            return Err(Error::Schema("YAML aliases are unsupported.".to_owned()));
        }
        // Aliases are rejected; unused anchor labels must not cause subtree copies.
        match &mut event {
            Event::Scalar(_, _, anchor, _)
            | Event::SequenceStart(anchor, _)
            | Event::MappingStart(anchor, _) => *anchor = 0,
            _ => {}
        }
        loader.on_event(event, span);
        if let Some(error) = loader.error() {
            return Err(Error::Schema(error.to_string()));
        }
        if opens_collection {
            depth += 1;
        }
        if closes_collection {
            depth = depth.saturating_sub(1);
        }
    }
    let documents = loader.into_documents();
    let value = match documents.as_slice() {
        [] => Value::Null,
        [document] => yaml_value(document, 0)?,
        _ => return Err(Error::Schema("Expected exactly one YAML document.".to_owned())),
    };
    crate::limits::validate_numbers(&value, 0)?;
    Ok(value)
}

fn yaml_value(node: &Yaml<'_>, depth: usize) -> Result<Value> {
    if depth > crate::limits::MAX_INPUT_DEPTH {
        return Err(Error::Schema("JSON input nesting exceeds 128 levels.".to_owned()));
    }
    match node {
        Yaml::Representation(source, style, tag) => {
            if tag.is_some() {
                return Err(Error::Schema(
                    "YAML tags are unsupported; use plain JSON-compatible data.".to_owned(),
                ));
            }
            if *style == ScalarStyle::Plain
                && let Ok(number) = serde_json::from_str::<Number>(source)
            {
                return Ok(Value::Number(number));
            }
            let scalar = Scalar::parse_from_cow_and_metadata(Cow::Borrowed(source), *style, None)
                .ok_or_else(|| Error::Schema("Invalid YAML scalar.".to_owned()))?;
            match scalar {
                Scalar::Null => Ok(Value::Null),
                Scalar::Boolean(value) => Ok(Value::Bool(value)),
                Scalar::Integer(value) => Ok(Value::Number(value.into())),
                Scalar::String(value) => Ok(Value::String(value.into_owned())),
                Scalar::FloatingPoint(_) => Err(Error::Schema(
                    "YAML numbers must use finite JSON number syntax.".to_owned(),
                )),
            }
        }
        Yaml::Sequence(values) => values.iter().map(|value| yaml_value(value, depth + 1)).collect(),
        Yaml::Mapping(values) => {
            let mut object = Map::with_capacity(values.len());
            for (key, value) in values {
                let Value::String(key) = yaml_value(key, depth + 1)? else {
                    return Err(Error::Schema("JSON object keys must be strings.".to_owned()));
                };
                if object.insert(key, yaml_value(value, depth + 1)?).is_some() {
                    return Err(Error::Schema("Duplicate YAML mapping key.".to_owned()));
                }
            }
            Ok(Value::Object(object))
        }
        _ => Err(Error::Schema("Unsupported YAML representation.".to_owned())),
    }
}
