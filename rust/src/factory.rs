//! Compiled contracts shared by independent generation batches.
use std::collections::BTreeSet;

use serde_json::Value;

use crate::cases::{self, Case};
use crate::engine::{Engine, GenerationOptions, ProviderContext};
use crate::minimize::{self, MinimizationResult};
use crate::node::{Compilation, Node};
use crate::{Dialect, Error, Result};

/// A compiled fixture contract; random and callback state belongs to each call.
#[derive(Debug)]
pub struct Factory {
    node: Node,
    schema: Value,
    names: BTreeSet<String>,
}

impl Factory {
    /// Compile a JSON Schema with built-in providers.
    ///
    /// # Errors
    /// Returns a schema error for invalid constraints or metadata.
    pub fn new(schema: &Value) -> Result<Self> {
        Self::with_providers(schema, BTreeSet::new())
    }

    /// Compile a schema with the names supplied to callback-based generation.
    ///
    /// # Errors
    /// Returns a schema error for invalid constraints or metadata.
    pub fn with_providers(schema: &Value, names: BTreeSet<String>) -> Result<Self> {
        let normalized = crate::prepare(schema, None, None)?;
        let node = Node::compile(&normalized, Dialect::default(), &Compilation { names: &names })?;
        Ok(Self { node, schema: normalized, names })
    }

    /// Generate a complete valid batch, with no partial return on exhaustion.
    ///
    /// # Errors
    /// Propagates provider errors and bounded generation exhaustion.
    pub fn generate(&self, options: &GenerationOptions) -> Result<Vec<Value>> {
        self.generate_with(options, &mut missing_callback)
    }

    /// Generate valid fixtures using caller-provided semantic generators.
    ///
    /// # Errors
    /// Propagates provider errors and bounded generation exhaustion.
    pub fn generate_with(
        &self,
        options: &GenerationOptions,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<Vec<Value>> {
        let mut engine = Engine::new(options, &self.names, provider)?;
        (0..options.count)
            .map(|index| {
                engine.generate(&self.node).map_err(|error| match error {
                    Error::Generation(message) => {
                        Error::Generation(format!("Fixture {index}: {message}"))
                    }
                    error => error,
                })
            })
            .collect()
    }

    /// Replace exactly one reachable failure declaration in each valid base.
    ///
    /// # Errors
    /// Fails when no declaration can produce changed JSON within the budget.
    pub fn failure_cases(&self, options: &GenerationOptions) -> Result<Vec<Case>> {
        self.failure_cases_with(options, &mut missing_callback)
    }

    /// Generate declared failures using caller-provided semantic generators.
    ///
    /// # Errors
    /// Reports missing declarations, callback failures, and bounded exhaustion.
    pub fn failure_cases_with(
        &self,
        options: &GenerationOptions,
        provider: &mut impl FnMut(&ProviderContext) -> Result<Value>,
    ) -> Result<Vec<Case>> {
        let mut engine = Engine::new(options, &self.names, provider)?;
        (0..options.count).map(|_| cases::failure(&self.node, &mut engine)).collect()
    }

    /// Reduce a reproducer while preserving the caller's observed failure.
    ///
    /// # Errors
    /// Rejects an invalid starting value and propagates oracle errors unchanged.
    pub fn minimize(
        &self,
        value: &Value,
        maximum: usize,
        preserve: bool,
        oracle: &mut impl FnMut(&Value) -> Result<bool>,
    ) -> Result<MinimizationResult> {
        minimize::minimize(&self.node, value, maximum, preserve, oracle)
    }

    /// Validate a typed model's serialized view against the original contract.
    #[must_use]
    pub fn is_valid(&self, value: &Value) -> bool {
        crate::limits::validate_numbers(value, 0).is_ok() && self.node.valid(value)
    }

    /// Return an isolated normalized schema for typed host-language adapters.
    #[must_use]
    pub fn schema(&self) -> Value {
        self.schema.clone()
    }
}

fn missing_callback(context: &ProviderContext) -> Result<Value> {
    Err(Error::Provider(format!(
        "Custom provider {} requires a generation callback.",
        context.name
    )))
}
