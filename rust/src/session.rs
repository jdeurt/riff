//! Complete JSON-facing operations shared by CLI and language interfaces.
use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Case, Document, Error, Factory, GenerationOptions, NumericMode, Operation, ProviderContext,
    Result,
};

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct Configuration {
    providers: BTreeSet<String>,
    operation: Option<Operation>,
    fixtures: Option<Document>,
    fixture_file: Option<std::path::PathBuf>,
}

/// An immutable fixture contract with complete JSON-facing operations.
#[derive(Debug)]
pub struct Session {
    factory: Factory,
    numbers: NumericMode,
}

#[derive(Serialize)]
struct PresentedCase<'a> {
    #[serde(flatten)]
    case: &'a Case,
    payload: &'a Value,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Expectation {
    #[default]
    Pass,
    Fail,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct MinimizeOptions {
    max_evaluations: usize,
    preserve_schema: bool,
}

impl Default for MinimizeOptions {
    fn default() -> Self {
        Self { max_evaluations: 100, preserve_schema: true }
    }
}

/// Validate generation controls and normalize signed seeds modulo 2^64.
///
/// # Errors
/// Rejects invalid argument types, bounds, profiles, seeds, and unknown keys.
pub fn validate_options(options: impl AsRef<str>) -> Result<GenerationOptions> {
    let options: GenerationOptions = controls(options.as_ref())?;
    options.validate()?;
    Ok(options)
}

fn controls<ControlsT: DeserializeOwned>(source: &str) -> Result<ControlsT> {
    let object: serde_json::Map<String, Value> =
        serde_json::from_str(source).map_err(|error| Error::Argument(error.to_string()))?;
    serde_json::from_value(Value::Object(object))
        .map_err(|error| Error::Argument(error.to_string()))
}

impl Session {
    /// Compile an inline or file contract with optional operation and fixture annotations.
    ///
    /// # Errors
    /// Rejects malformed inputs, unsupported contracts, and lossy numeric representations.
    pub fn new(
        source: impl AsRef<str>,
        configuration: impl AsRef<str>,
        numbers: NumericMode,
    ) -> Result<Self> {
        let source: Document = serde_json::from_str(source.as_ref())?;
        let object: serde_json::Map<String, Value> = serde_json::from_str(configuration.as_ref())?;
        let configuration: Configuration = serde_json::from_value(Value::Object(object))?;
        let document = source.load()?;
        numbers.validate(&document)?;
        if configuration.operation.is_none() && !document.is_object() && !document.is_boolean() {
            return Err(Error::Schema(
                "Standalone schema must be an object or boolean.".to_owned(),
            ));
        }
        if configuration.fixtures.is_some() && configuration.fixture_file.is_some() {
            return Err(Error::Schema("Choose fixtures or fixtureFile, not both.".to_owned()));
        }
        let fixture_source =
            configuration.fixtures.or_else(|| configuration.fixture_file.map(Document::File));
        let fixtures = fixture_source.as_ref().map(Document::load).transpose()?;
        if let Some(fixtures) = &fixtures {
            numbers.validate(fixtures)?;
        }
        let schema =
            crate::prepare(&document, configuration.operation.as_ref(), fixtures.as_ref())?;
        let factory = Factory::with_providers(&schema, configuration.providers)?;
        Ok(Self { factory, numbers })
    }

    /// Generate a complete JSON array of schema-valid fixtures.
    ///
    /// # Errors
    /// Reports invalid controls, provider failures, search exhaustion, and numeric rounding.
    pub fn generate(
        &self,
        options: impl AsRef<str>,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<String> {
        let options = self.options(options.as_ref())?;
        let values = self.factory.generate_with(&options, provider)?;
        self.encode(&serde_json::to_value(values)?)
    }

    /// Generate labeled cases with both original payloads and declared outcomes.
    ///
    /// # Errors
    /// Reports invalid controls, missing failure declarations, and generation failures.
    pub fn cases(
        &self,
        options: impl AsRef<str>,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<String> {
        let mut value: Value = serde_json::from_str(options.as_ref())
            .map_err(|error| Error::Argument(error.to_string()))?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| Error::Argument("Options must be an object.".to_owned()))?;
        let expectation: Expectation = object
            .remove("expectation")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| Error::Argument(error.to_string()))?
            .unwrap_or_default();
        let options = self.options(&value.to_string())?;
        self.draw_cases(&options, expectation, provider)
    }

    /// Generate successful cases with a schema-validity guarantee.
    ///
    /// # Errors
    /// Reports invalid controls, provider failures, and generation exhaustion.
    pub fn success_cases(
        &self,
        options: impl AsRef<str>,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<String> {
        self.draw_cases(&self.options(options.as_ref())?, Expectation::Pass, provider)
    }

    /// Generate explicit failure replacements and whole-payload diagnostics.
    ///
    /// # Errors
    /// Reports invalid controls, provider failures, and unreachable failure declarations.
    pub fn failure_cases(
        &self,
        options: impl AsRef<str>,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<String> {
        self.draw_cases(&self.options(options.as_ref())?, Expectation::Fail, provider)
    }

    fn draw_cases(
        &self,
        options: &GenerationOptions,
        expectation: Expectation,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<String> {
        let cases = match expectation {
            Expectation::Pass => self
                .factory
                .generate_with(options, provider)?
                .into_iter()
                .map(|value| Case {
                    value,
                    expectation: "pass".to_owned(),
                    schema_valid: true,
                    violations: Vec::new(),
                    target: None,
                    reason: None,
                })
                .collect(),
            Expectation::Fail => self.factory.failure_cases_with(options, provider)?,
        };
        self.encode(&serde_json::to_value(
            cases
                .iter()
                .map(|case| PresentedCase { case, payload: &case.value })
                .collect::<Vec<_>>(),
        )?)
    }

    /// Minimize a reproducer with a synchronous, caller-owned failure oracle.
    ///
    /// # Errors
    /// Rejects invalid controls or starts and propagates oracle failures.
    pub fn minimize(
        &self,
        value: impl AsRef<str>,
        options: impl AsRef<str>,
        oracle: &mut impl FnMut(&Value) -> Result<bool>,
    ) -> Result<String> {
        let input: Value = serde_json::from_str(value.as_ref())?;
        self.numbers.validate(&input)?;
        let options: MinimizeOptions = controls(options.as_ref())?;
        let result = self.factory.minimize(
            &input,
            options.max_evaluations,
            options.preserve_schema,
            &mut |candidate| {
                self.numbers
                    .validate(candidate)
                    .map_err(|error| Error::Generation(error.to_string()))?;
                oracle(candidate)
            },
        )?;
        self.encode(&serde_json::to_value(result)?)
    }

    /// Validate a supplied JSON value against the original fixture contract.
    ///
    /// # Errors
    /// Rejects malformed JSON or unsupported numeric representations.
    pub fn is_valid(&self, value: impl AsRef<str>) -> Result<bool> {
        let value: Value = serde_json::from_str(value.as_ref())?;
        self.numbers.validate(&value)?;
        Ok(self.factory.is_valid(&value))
    }

    fn options(&self, source: &str) -> Result<GenerationOptions> {
        let value: Value =
            serde_json::from_str(source).map_err(|error| Error::Argument(error.to_string()))?;
        if matches!(self.numbers, NumericMode::SafeFloat)
            && let Some(seed @ Value::Number(_)) = value.get("seed")
        {
            self.numbers.validate(seed).map_err(|error| Error::Argument(error.to_string()))?;
        }
        validate_options(source)
    }

    fn encode(&self, value: &Value) -> Result<String> {
        self.numbers.validate(value).map_err(|error| {
            Error::Generation(format!("JSON consumer cannot preserve numeric output: {error}"))
        })?;
        serde_json::to_string(value).map_err(Into::into)
    }
}
