//! Per-batch random state and bounded candidate generation.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::metadata::ConstraintMode;
use crate::mutations;
use crate::node::{Compilation, Kind, Node, SchemaNode, Types, intersect};
use crate::scalars::{self, choose};
use crate::schema::{Dialect, prepare};
use crate::{Error, Metadata, Result, Source, Strategy};

pub const MAX_COUNT: usize = 10_000;
pub const MAX_ATTEMPTS: usize = 100_000;
pub const MAX_SEED_DIGITS: u16 = 1024;

/// Sampling preference; every successful value still obeys the schema.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Mix semantic data and structural exploration.
    #[default]
    Mixed,
    /// Prefer declared providers.
    Realistic,
    /// Emphasize small and endpoint values.
    Boundary,
}

/// Per-batch generation controls.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct GenerationOptions {
    /// Number of fixtures, capped at 10,000 per batch.
    pub count: usize,
    /// Unsigned 64-bit seed, or fresh randomness when omitted.
    #[serde(deserialize_with = "deserialize_seed")]
    pub seed: Option<u64>,
    /// Sampling preference.
    pub profile: Profile,
    /// Per-draw rejection limit, capped at 100,000.
    pub max_attempts: usize,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self { count: 1, seed: None, profile: Profile::Mixed, max_attempts: 100 }
    }
}

impl GenerationOptions {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.count > MAX_COUNT || self.max_attempts == 0 || self.max_attempts > MAX_ATTEMPTS {
            return Err(Error::Argument(
                "count must be 0..10000 and maxAttempts must be 1..100000.".to_owned(),
            ));
        }
        Ok(())
    }
}

/// A host request for a semantic provider or schema-required string format.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// Registered semantic name, allowing overrides of built-in defaults.
    Provider,
    /// A recognized JSON Schema string format.
    Format,
}

/// Fresh, isolated inputs for exactly one provider invocation.
#[derive(Clone, Debug)]
pub struct ProviderContext {
    /// Whether this request names a semantic provider or string format.
    pub kind: ProviderKind,
    /// Portable semantic name or recognized format.
    pub name: String,
    /// Lossless per-invocation seed.
    pub seed: u64,
    /// Requested locale.
    pub locale: String,
    /// Normalized enclosing field schema.
    pub schema: Value,
    /// Provider-specific JSON options.
    pub options: Map<String, Value>,
}

impl Serialize for ProviderContext {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut object = serializer.serialize_struct("ProviderContext", 6)?;
        object.serialize_field("kind", &self.kind)?;
        object.serialize_field("name", &self.name)?;
        object.serialize_field("seed", &self.seed.to_string())?;
        object.serialize_field("locale", &self.locale)?;
        object.serialize_field("schema", &self.schema)?;
        object.serialize_field("options", &self.options)?;
        object.end()
    }
}

pub struct Engine<'a, F: FnMut(&ProviderContext) -> Result<Value>> {
    pub random: ChaCha8Rng,
    pub max_attempts: usize,
    profile: Profile,
    smaller: bool,
    compiled: BTreeMap<crate::Dialect, HashMap<Value, Node>>,
    remaining: usize,
    names: &'a BTreeSet<String>,
    provider: &'a mut F,
}

impl<'a, F: FnMut(&ProviderContext) -> Result<Value>> Engine<'a, F> {
    pub fn new(
        options: &GenerationOptions,
        names: &'a BTreeSet<String>,
        provider: &'a mut F,
    ) -> Result<Self> {
        options.validate()?;
        let seed = options.seed.unwrap_or_else(|| rand::rng().next_u64());
        Ok(Self {
            random: ChaCha8Rng::seed_from_u64(seed),
            max_attempts: options.max_attempts,
            profile: options.profile,
            smaller: false,
            compiled: BTreeMap::new(),
            remaining: 0,
            names,
            provider,
        })
    }
    pub const fn reset(&mut self) {
        self.remaining = self.max_attempts * 100;
    }
    pub fn generate(&mut self, node: &Node) -> Result<Value> {
        let mut budget = self.max_attempts * 100;
        let mut allowance = 128;
        let mut last = String::new();
        for attempt in 0..self.max_attempts {
            self.smaller = attempt >= 4;
            let allocated = allowance.min(budget);
            self.remaining = allocated;
            let candidate = self.validated(node, 0);
            budget -= allocated - self.remaining;
            match candidate {
                Ok(value) => return Ok(value),
                Err(Error::Generation(message)) if budget > 0 => last = message,
                Err(error) => return Err(error),
            }
            allowance = allowance.saturating_mul(2).min(budget);
        }
        Err(Error::Generation(format!(
            "No valid fixture after {} attempts: {last}",
            self.max_attempts
        )))
    }
    fn spend(&mut self, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(Error::Generation("Generation exceeded depth budget.".to_owned()));
        }
        if self.remaining == 0 {
            return Err(Error::Generation("Generation exceeded candidate-work budget.".to_owned()));
        }
        self.remaining -= 1;
        Ok(())
    }
    fn validated(&mut self, node: &Node, depth: usize) -> Result<Value> {
        if depth > 32 {
            return Err(Error::Generation("Generation exceeded depth budget.".to_owned()));
        }
        if !node.can_sample() {
            return Err(Error::Generation("The false schema has no valid fixtures.".to_owned()));
        }
        let declared = matches!(node, Node::Schema(node) if node.metadata.source.provider.is_some() || node.metadata.source.examples.is_some() || node.metadata.source.strategy.is_some());
        let attempts = if depth == 0 {
            1
        } else if declared {
            self.max_attempts
        } else {
            self.max_attempts.min(8)
        };
        let mut last = String::new();
        for _ in 0..attempts {
            let candidate = self.candidate(node, depth);
            match candidate {
                Ok(candidate)
                    if scalars::within_limits(&candidate, 0) && node.valid(&candidate) =>
                {
                    return Ok(candidate);
                }
                Ok(candidate) => {
                    last = if !scalars::within_limits(&candidate, 0) {
                        "Candidate exceeds JSON payload resource limits.".to_owned()
                    } else if let Node::Schema(schema) = node {
                        schema.validator.iter_errors(&candidate).next().map_or_else(
                            || "Candidate does not satisfy the schema.".to_owned(),
                            |error| error.to_string(),
                        )
                    } else {
                        "Candidate does not satisfy the schema.".to_owned()
                    };
                }
                Err(Error::Generation(message)) if self.remaining > 0 => last = message,
                Err(error) => return Err(error),
            }
        }
        Err(Error::Generation(format!("No valid fixture after {attempts} attempts: {last}")))
    }
    fn candidate(&mut self, node: &Node, depth: usize) -> Result<Value> {
        self.spend(depth)?;
        let node = match node {
            Node::Boolean(false) => {
                return Err(Error::Generation(
                    "The false schema has no valid fixtures.".to_owned(),
                ));
            }
            Node::Boolean(true) => return self.unconstrained(depth),
            Node::Schema(node) => node,
        };
        let source = &node.metadata.source;
        if let Some(strategy) = &source.strategy {
            return self.strategy(strategy, node, depth);
        }
        if let Some(values) = &source.examples
            && self.random.random_range(0..3) == 0
        {
            return choose(values, &mut self.random).cloned();
        }
        if node.constant_present {
            return Ok(node.data.constant.clone());
        }
        if let Some(values) = &node.data.enumeration {
            return choose(values, &mut self.random).cloned();
        }
        if source.provider.is_some()
            && (self.profile == Profile::Realistic
                || (self.profile == Profile::Mixed && self.random.random::<bool>()))
        {
            return self.source(source, node, depth);
        }
        self.structural(node, depth)
    }
    fn unconstrained(&mut self, depth: usize) -> Result<Value> {
        if depth < 5 && self.random.random_range(0..3) == 0 {
            let values = (0..self.random.random_range(0..3))
                .map(|_| self.candidate(&Node::Boolean(true), depth + 1))
                .collect::<Result<Vec<_>>>()?;
            return Ok(if self.random.random::<bool>() {
                Value::Array(values)
            } else {
                Value::Object(
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| (format!("nested_{index}"), value))
                        .collect(),
                )
            });
        }
        choose(
            &[
                Value::Null,
                json!(false),
                json!(true),
                json!(0),
                json!(-1),
                json!(42),
                json!(""),
                json!("unexpected"),
                json!("🦊"),
                json!([]),
                json!({}),
            ],
            &mut self.random,
        )
        .cloned()
    }
    fn structural(&mut self, node: &SchemaNode, depth: usize) -> Result<Value> {
        if let Some(condition) = &node.condition {
            let positive = self.random.random::<bool>();
            let mut base = node.clone();
            base.condition = None;
            base.then_schema = None;
            base.else_schema = None;
            let condition = if positive {
                condition.clone()
            } else {
                self.compile(&json!({"not": condition.sampling_schema()}), node.dialect)?
            };
            let branch = if positive { &node.then_schema } else { &node.else_schema };
            let hint = intersect(&Node::Schema(Arc::new(base)), &condition)?;
            let hint = intersect(&hint, branch.as_ref().unwrap_or(&Node::Boolean(true)))?;
            return self.candidate(&hint, depth + 1);
        }
        if !node.all_of.is_empty() {
            let mut base = node.clone();
            base.all_of.clear();
            let mut hint = Node::Schema(Arc::new(base));
            for branch in &node.all_of {
                hint = intersect(&hint, branch)?;
            }
            return self.candidate(&hint, depth + 1);
        }
        if !node.any_of.is_empty() || !node.one_of.is_empty() {
            let mut base = node.clone();
            base.any_of.clear();
            base.one_of.clear();
            let mut hint = Node::Schema(Arc::new(base));
            for group in [&node.any_of, &node.one_of] {
                if !group.is_empty() {
                    hint = intersect(&hint, choose(group, &mut self.random)?)?;
                }
            }
            return self.candidate(&hint, depth + 1);
        }
        let kind = match &node.data.kind {
            Some(Types::Single(kind)) => *kind,
            Some(Types::Multiple(kinds)) => *choose(kinds, &mut self.random)?,
            None if !node.properties.is_empty() || !node.data.required.is_empty() => Kind::Object,
            None if node.applied.items || !node.tuple.is_empty() || node.contains.is_some() => {
                Kind::Array
            }
            None => *choose(
                &[
                    Kind::Null,
                    Kind::Boolean,
                    Kind::Integer,
                    Kind::String,
                    Kind::Array,
                    Kind::Object,
                ],
                &mut self.random,
            )?,
        };
        match kind {
            Kind::Null => Ok(Value::Null),
            Kind::Boolean => Ok(json!(self.random.random::<bool>())),
            Kind::Integer | Kind::Number => scalars::number(
                node,
                kind == Kind::Integer,
                self.profile == Profile::Boundary,
                &mut self.random,
            ),
            Kind::String => self.string(node),
            Kind::Array => self.array(node, depth),
            Kind::Object => self.object(node, depth),
        }
    }
    pub fn source(&mut self, source: &Source, node: &SchemaNode, depth: usize) -> Result<Value> {
        if let Some(strategy) = &source.strategy {
            return self.strategy(strategy, node, depth);
        }
        if let Some(values) = &source.examples
            && (source.provider.is_none() || self.random.random::<bool>())
        {
            return choose(values, &mut self.random).cloned();
        }
        let name = source
            .provider
            .as_ref()
            .ok_or_else(|| Error::Generation("No candidate source is declared.".to_owned()))?;
        self.invoke(
            ProviderKind::Provider,
            name,
            source.locale.as_deref().unwrap_or("en-US"),
            source.options.clone().unwrap_or_default(),
            node,
        )
    }
    fn strategy(&mut self, strategy: &Strategy, node: &SchemaNode, depth: usize) -> Result<Value> {
        self.spend(depth)?;
        match strategy {
            Strategy::Schema { constraints: ConstraintMode::Violate } => {
                choose(&mutations::violations(node)?, &mut self.random).cloned()
            }
            Strategy::Schema { constraints: ConstraintMode::Respect } => {
                let mut structural = node.clone();
                structural.metadata = Metadata::default();
                self.validated(&Node::Schema(Arc::new(structural)), depth + 1)
            }
            Strategy::Values { values } => choose(values, &mut self.random).cloned(),
            Strategy::Provider { name, locale, options } => {
                self.invoke(ProviderKind::Provider, name, locale, options.clone(), node)
            }
            Strategy::Choice { strategies } => {
                let child = choose(strategies, &mut self.random)?;
                self.strategy(child, node, depth + 1)
            }
            Strategy::Filter { schema, strategy } => {
                let normalized = prepare(schema, None, None)?;
                let predicate = self.compile(&normalized, node.dialect)?;
                for _ in 0..self.max_attempts {
                    let value = self.strategy(strategy, node, depth + 1)?;
                    if predicate.valid(&value) {
                        return Ok(value);
                    }
                }
                Err(Error::Generation("Strategy filter exhausted its rejection budget.".to_owned()))
            }
        }
    }
    fn compile(&mut self, value: &Value, dialect: Dialect) -> Result<Node> {
        if let Some(node) = self.compiled.get(&dialect).and_then(|schemas| schemas.get(value)) {
            return Ok(node.clone());
        }
        let node = Node::compile(value, dialect, &Compilation { names: self.names })?;
        let _ = self.compiled.entry(dialect).or_default().insert(value.clone(), node.clone());
        Ok(node)
    }
    fn invoke(
        &mut self,
        kind: ProviderKind,
        name: &str,
        locale: &str,
        options: Map<String, Value>,
        node: &SchemaNode,
    ) -> Result<Value> {
        let custom = matches!(kind, ProviderKind::Provider) && self.names.contains(name);
        let context = ProviderContext {
            kind,
            name: name.to_owned(),
            seed: self.random.next_u64(),
            locale: locale.to_owned(),
            schema: if custom { node.declared.render()? } else { node.sampling_schema.clone() },
            options,
        };
        let value =
            if custom { (self.provider)(&context) } else { crate::providers::generate(&context) }
                .map_err(|error| Error::Provider(format!("Provider {name} failed: {error}")))?;
        crate::limits::validate_numbers(&value, 0).map_err(|error| {
            Error::Provider(format!("Provider {name} returned unsupported numeric data: {error}"))
        })?;
        Ok(value)
    }
    fn string(&mut self, node: &SchemaNode) -> Result<Value> {
        if let Some(format) = &node.data.format
            && crate::providers::FORMATS.contains(&format.as_str())
        {
            return self.invoke(ProviderKind::Format, format, "en-US", Map::new(), node);
        }
        scalars::string(
            node,
            self.profile == Profile::Boundary,
            self.profile == Profile::Realistic,
            &mut self.random,
        )
        .map(Value::String)
    }
    fn array(&mut self, node: &SchemaNode, depth: usize) -> Result<Value> {
        let contains_minimum = if node.contains.is_none() {
            0
        } else if node.dialect == Dialect::Draft7 {
            1
        } else {
            node.data.min_contains.unwrap_or(1)
        };
        let closed = (node.sampling_schema.get("items").is_some_and(Value::is_array)
            && node.additional_items.is_false())
            || (node.applied.items && node.items.is_false())
            || (!node.applied.items
                && !node.applied.additional_items
                && node.unevaluated_items.as_ref().is_some_and(Node::is_false)
                && node.contains.is_none());
        let maximum = if closed {
            Some(node.data.max_items.unwrap_or(node.tuple.len()).min(node.tuple.len()))
        } else {
            node.data.max_items
        };
        let length = if self.smaller {
            node.data.min_items.max(contains_minimum)
        } else {
            scalars::size(
                node.data.min_items.max(contains_minimum),
                maximum,
                5 / (depth + 1),
                self.profile == Profile::Boundary,
                &mut self.random,
            )?
        };
        let mut values = Vec::new();
        for index in 0..length {
            let mut found = None;
            for _ in 0..self.max_attempts {
                let value = self.validated(&node.item(index), depth + 1)?;
                if !node.data.unique_items
                    || !values.iter().any(|previous| crate::cases::equivalent(previous, &value))
                {
                    found = Some(value);
                    break;
                }
            }
            values.push(found.ok_or_else(|| {
                Error::Generation("Unable to generate enough distinct array items.".to_owned())
            })?);
        }
        if let Some(contains) = &node.contains {
            let maximum =
                if node.dialect == Dialect::Draft7 { None } else { node.data.max_contains };
            for index in 0..values.len() {
                let matches = values.iter().filter(|value| contains.valid(value)).count();
                let value = values
                    .get(index)
                    .ok_or_else(|| Error::Generation("Array index is unreachable.".to_owned()))?;
                let too_few = matches < contains_minimum && !contains.valid(value);
                let too_many =
                    maximum.is_some_and(|maximum| matches > maximum) && contains.valid(value);
                if !too_few && !too_many {
                    continue;
                }
                let desired = if too_few {
                    contains.clone()
                } else {
                    self.compile(&json!({"not": contains.sampling_schema()}), node.dialect)?
                };
                let replacement =
                    self.validated(&intersect(&node.item(index), &desired)?, depth + 1)?;
                if let Some(value) = values.get_mut(index) {
                    *value = replacement;
                }
            }
        }
        Ok(Value::Array(values))
    }
    fn object(&mut self, node: &SchemaNode, depth: usize) -> Result<Value> {
        let mut selected: BTreeSet<_> = node.data.required.iter().cloned().collect();
        let mut optional: Vec<_> = node
            .properties
            .iter()
            .filter(|(name, child)| !selected.contains(*name) && child.can_sample())
            .map(|(name, _)| name.clone())
            .collect();
        let extra_allowed = !node.additional.is_false()
            && (!node.unevaluated_properties.as_ref().is_some_and(Node::is_false)
                || node.applied.additional);
        let minimum = node.data.min_properties.max(selected.len());
        let default = minimum.max(
            node.properties.len().div_ceil(depth + 1)
                + if extra_allowed || !node.patterns.is_empty() { 2 } else { 0 },
        );
        let wanted = if self.smaller {
            minimum
        } else {
            scalars::size(
                minimum,
                node.data.max_properties,
                default,
                self.profile == Profile::Boundary,
                &mut self.random,
            )?
        };
        while !optional.is_empty() && selected.len() < wanted {
            let index = self.random.random_range(0..optional.len());
            let _ = selected.insert(optional.remove(index));
        }
        for _ in 0..=node.required_dependencies.len() {
            self.spend(depth)?;
            let additions: Vec<_> = selected
                .iter()
                .flat_map(|name| node.required_dependencies.get(name).into_iter().flatten())
                .cloned()
                .collect();
            let before = selected.len();
            selected.extend(additions);
            if selected.len() == before {
                break;
            }
        }
        let triggered: HashSet<_> =
            node.dependencies.keys().filter(|name| selected.contains(*name)).cloned().collect();
        if !triggered.is_empty() {
            let mut base = node.clone();
            base.data.required = selected.iter().cloned().collect();
            base.dependencies.retain(|name, _| !triggered.contains(name));
            let mut hint = Node::Schema(Arc::new(base));
            for (name, child) in &node.dependencies {
                if triggered.contains(name) {
                    hint = intersect(&hint, child)?;
                }
            }
            self.spend(depth + 1)?;
            return match hint {
                Node::Schema(node) => self.object(&node, depth + 1),
                Node::Boolean(_) => {
                    Err(Error::Generation("Triggered dependency has no object values.".to_owned()))
                }
            };
        }
        let mut values = Map::new();
        for name in selected {
            let _ = values
                .insert(name.clone(), self.validated(&Self::property(node, &name)?, depth + 1)?);
        }
        if extra_allowed || !node.patterns.is_empty() {
            for index in 0..self.max_attempts {
                if values.len() >= wanted {
                    break;
                }
                self.spend(depth)?;
                let name = self.extra_name(node, index, depth)?;
                if values.contains_key(&name) || node.properties.contains_key(&name) {
                    continue;
                }
                let child = Self::property(node, &name)?;
                if child.can_sample() {
                    let _ = values.insert(name, self.validated(&child, depth + 1)?);
                }
            }
        }
        Ok(Value::Object(values))
    }
    fn property(node: &SchemaNode, name: &str) -> Result<Node> {
        let patterns = node.matching_patterns(name)?;
        if !node.properties.contains_key(name) && patterns.is_empty() {
            return Ok(if node.applied.additional {
                node.additional.clone()
            } else {
                node.unevaluated_properties.clone().unwrap_or_else(|| node.additional.clone())
            });
        }
        let mut hint = node.properties.get(name).cloned().unwrap_or(Node::Boolean(true));
        for child in patterns {
            hint = intersect(&hint, child)?;
        }
        Ok(hint)
    }
    fn extra_name(&mut self, node: &SchemaNode, index: usize, depth: usize) -> Result<String> {
        if !node.patterns.is_empty() && (node.additional.is_false() || index.is_multiple_of(2)) {
            return scalars::regex_value(
                &choose(&node.patterns, &mut self.random)?.source,
                &mut self.random,
            );
        }
        if let Some(names) = &node.names {
            let string = self.compile(&json!({"type": "string"}), node.dialect)?;
            let value = self.validated(&intersect(&string, names)?, depth + 1)?;
            return match value {
                Value::String(name) => Ok(name),
                _ => Err(Error::Generation("propertyNames did not produce a string.".to_owned())),
            };
        }
        Ok(format!("extra_{}", index + node.properties.len()))
    }
}

fn deserialize_seed<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<u64>, D::Error> {
    use num_bigint::BigInt;
    use num_integer::Integer;
    use std::str::FromStr;

    let value = Value::deserialize(deserializer)?;
    let text = match value {
        Value::Null => return Ok(None),
        Value::String(text) => text,
        Value::Number(number) => number.to_string(),
        _ => {
            return Err(serde::de::Error::custom(
                "seed must be an integer or decimal integer string",
            ));
        }
    };
    let digits = text.strip_prefix(['+', '-']).unwrap_or(&text);
    if digits.len() > usize::from(MAX_SEED_DIGITS) || text.contains(['.', 'e', 'E']) {
        return Err(serde::de::Error::custom("seed must be an integer with at most 1024 digits"));
    }
    let seed = BigInt::from_str(&text).map_err(serde::de::Error::custom)?;
    let modulus = BigInt::from(1_u128 << 64);
    u64::try_from(seed.mod_floor(&modulus)).map(Some).map_err(serde::de::Error::custom)
}
