//! Portable source and strategy declarations.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Error, Result};

/// A portable strategy tree; no host-language expressions are evaluated.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Strategy {
    /// Generate from the enclosing schema, optionally violating it in failure sources.
    #[serde(rename = "schema")]
    Schema {
        /// Respect constraints unless explicitly marked as a negative source.
        #[serde(default)]
        constraints: ConstraintMode,
    },
    /// Choose one explicit JSON value.
    #[serde(rename = "values")]
    Values {
        /// A nonempty corpus.
        values: Vec<Value>,
    },
    /// Invoke one named provider.
    #[serde(rename = "provider")]
    Provider {
        /// Registered semantic name.
        name: String,
        /// Requested language and region.
        #[serde(default = "default_locale")]
        locale: String,
        /// Isolated provider-specific JSON options.
        #[serde(default)]
        options: Map<String, Value>,
    },
    /// Choose a child strategy.
    #[serde(rename = "one-of")]
    Choice {
        /// Nonempty alternatives.
        strategies: Vec<Self>,
    },
    /// Reject child values that fail an independent schema predicate.
    #[serde(rename = "filter")]
    Filter {
        /// Predicate schema, resolved in its own local reference scope.
        schema: Value,
        /// Candidate source.
        strategy: Box<Self>,
    },
}

/// Whether a schema strategy respects or deliberately violates constraints.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintMode {
    /// Produce valid data.
    #[default]
    Respect,
    /// Produce a locally invalid replacement in an explicit failure source.
    Violate,
}

/// Shorthand or explicit candidate source.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Registered provider name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Requested provider locale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Provider options; absent differs from an explicitly supplied empty object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Map<String, Value>>,
    /// Nonempty example values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<Value>>,
    /// Explicit strategy, mutually exclusive with shorthand fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
    /// Caller-owned explanation for a negative expectation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Inline positive source and independent failure declaration.
#[derive(Clone, Debug, Default)]
pub struct Metadata {
    /// Positive candidate source.
    pub source: Source,
    /// Deliberate negative candidate source.
    pub failure: Option<Source>,
}

pub const BUILTINS: &[&str] =
    &["email", "full-name", "company-name", "url", "phone-number", "city", "country-code", "uuid"];

pub fn default_locale() -> String {
    "en-US".to_owned()
}

impl Metadata {
    /// Parse metadata before generation without importing provider code.
    ///
    /// # Errors
    /// Returns a schema error for unknown fields, invalid combinations, or null sources.
    pub fn parse(value: Option<&Value>) -> Result<Self> {
        let Some(value) = value else { return Ok(Self::default()) };
        let Value::Object(mut object) = value.clone() else {
            return Err(Error::Schema("x-fixture must be an object.".to_owned()));
        };
        let failure = object.remove("fail");
        reject_nulls(&object)?;
        let source: Source = crate::documents::decode(&Value::Object(object))?;
        if source.reason.is_some() {
            return Err(Error::Schema("reason belongs inside fail.".to_owned()));
        }
        source.check(false)?;
        let failure = failure
            .map(|input| {
                let Value::Object(ref object) = input else {
                    return Err(Error::Schema("fail must be an object.".to_owned()));
                };
                reject_nulls(object)?;
                let parsed: Source = crate::documents::decode(&input)?;
                parsed.check(true)?;
                Ok(parsed)
            })
            .transpose()?;
        Ok(Self { source, failure })
    }
}

fn reject_nulls(object: &Map<String, Value>) -> Result<()> {
    if object.values().any(Value::is_null) {
        return Err(Error::Schema("Source fields must not be null.".to_owned()));
    }
    Ok(())
}

impl Source {
    pub(crate) fn check(&self, failure: bool) -> Result<()> {
        let shorthand = self.provider.is_some()
            || self.locale.is_some()
            || self.options.is_some()
            || self.examples.is_some();
        if self.strategy.is_some() && shorthand {
            return Err(Error::Schema(
                "strategy cannot coexist with shorthand source keys.".to_owned(),
            ));
        }
        if self.provider.is_none() && (self.locale.is_some() || self.options.is_some()) {
            return Err(Error::Schema("locale and options require a provider.".to_owned()));
        }
        if self.examples.as_ref().is_some_and(Vec::is_empty) {
            return Err(Error::Schema("examples must be nonempty.".to_owned()));
        }
        if failure && self.provider.is_none() && self.examples.is_none() && self.strategy.is_none()
        {
            return Err(Error::Schema("fail requires provider, examples, or strategy.".to_owned()));
        }
        if let Some(strategy) = &self.strategy {
            strategy.check(failure, 0)?;
        }
        Ok(())
    }

    pub(crate) fn validate_names(&self, names: &BTreeSet<String>) -> Result<()> {
        if let Some(name) = &self.provider {
            provider_name(name, names)?;
        }
        if let Some(strategy) = &self.strategy {
            strategy.validate_names(names)?;
        }
        Ok(())
    }
}

fn provider_name(name: &str, names: &BTreeSet<String>) -> Result<()> {
    if !names.contains(name) && !BUILTINS.contains(&name) {
        return Err(Error::Schema(format!("Unknown provider: {name}")));
    }
    Ok(())
}

impl Strategy {
    fn check(&self, failure: bool, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(Error::Schema("Strategy depth exceeds 32.".to_owned()));
        }
        match self {
            Self::Schema { constraints: ConstraintMode::Violate } if !failure => {
                return Err(Error::Schema(
                    "constraints: violate requires an explicit failure source.".to_owned(),
                ));
            }
            Self::Values { values } if values.is_empty() => {
                return Err(Error::Schema("values must be nonempty.".to_owned()));
            }
            Self::Choice { strategies } => {
                if strategies.is_empty() {
                    return Err(Error::Schema("strategies must be nonempty.".to_owned()));
                }
                for child in strategies {
                    child.check(failure, depth + 1)?;
                }
            }
            Self::Filter { strategy, .. } => strategy.check(failure, depth + 1)?,
            _ => {}
        }
        Ok(())
    }
    fn validate_names(&self, names: &BTreeSet<String>) -> Result<()> {
        match self {
            Self::Provider { name, .. } => provider_name(name, names)?,
            Self::Choice { strategies } => {
                for child in strategies {
                    child.validate_names(names)?;
                }
            }
            Self::Filter { strategy, .. } => strategy.validate_names(names)?,
            _ => {}
        }
        Ok(())
    }
}
