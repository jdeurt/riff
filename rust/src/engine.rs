//! Per-batch random state and bounded candidate generation.
use std::collections::{BTreeSet, HashSet};

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

/// Validated invocation options. Seed strings carry all 64 bits across bindings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct GenerationOptions {
    /// Number of fixtures, capped at 10,000 per batch.
    pub count: usize,
    /// Decimal unsigned 64-bit seed, or fresh randomness when omitted.
    pub seed: Option<String>,
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
        if self.count > 10_000 || self.max_attempts == 0 || self.max_attempts > 100_000 {
            return Err(Error::Schema(
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
    /// Dispatch used internally by host-language default adapters.
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
        let mut object = serializer.serialize_struct("ProviderContext", 7)?;
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
    remaining: usize,
    names: &'a BTreeSet<String>,
    locales: &'a BTreeSet<String>,
    provider: &'a mut F,
}

impl<'a, F: FnMut(&ProviderContext) -> Result<Value>> Engine<'a, F> {
    pub fn new(
        options: &GenerationOptions,
        names: &'a BTreeSet<String>,
        locales: &'a BTreeSet<String>,
        provider: &'a mut F,
    ) -> Result<Self> {
        options.validate()?;
        let seed = options.seed.as_ref().map_or_else(
            || Ok(rand::rng().next_u64()),
            |seed| {
                seed.parse::<u64>().map_err(|error| Error::Schema(format!("Invalid seed: {error}")))
            },
        )?;
        Ok(Self {
            random: ChaCha8Rng::seed_from_u64(seed),
            max_attempts: options.max_attempts,
            profile: options.profile,
            remaining: 0,
            names,
            locales,
            provider,
        })
    }
    pub const fn reset(&mut self) {
        self.remaining = self.max_attempts * 100;
    }
    pub fn generate(&mut self, node: &Node) -> Result<Value> {
        self.reset();
        self.validated(node, 0)
    }
    fn spend(&mut self, depth: usize) -> Result<()> {
        if depth > 32 || self.remaining == 0 {
            return Err(Error::Generation(
                "Generation exceeded candidate or depth budget.".to_owned(),
            ));
        }
        self.remaining -= 1;
        Ok(())
    }
    fn validated(&mut self, node: &Node, depth: usize) -> Result<Value> {
        let mut last = String::new();
        for _ in 0..self.max_attempts {
            match self.candidate(node, depth) {
                Ok(candidate)
                    if scalars::within_limits(&candidate, 0) && node.valid(&candidate) =>
                {
                    return Ok(candidate);
                }
                Ok(_) => {}
                Err(Error::Generation(message)) if self.remaining > 0 => last = message,
                Err(error) => return Err(error),
            }
        }
        Err(Error::Generation(format!(
            "No valid fixture after {} attempts: {last}",
            self.max_attempts
        )))
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
                self.compile(&json!({"not": condition.contract()}), node.dialect)?
            };
            let branch = if positive { &node.then_schema } else { &node.else_schema };
            let hint = intersect(&Node::Schema(Box::new(base)), &condition)?;
            let hint = intersect(&hint, branch.as_ref().unwrap_or(&Node::Boolean(true)))?;
            return self.candidate(&hint, depth + 1);
        }
        if !node.all_of.is_empty() {
            let mut base = node.clone();
            base.all_of.clear();
            let mut hint = Node::Schema(Box::new(base));
            for branch in &node.all_of {
                hint = intersect(&hint, branch)?;
            }
            return self.candidate(&hint, depth + 1);
        }
        if !node.any_of.is_empty() || !node.one_of.is_empty() {
            let mut base = node.clone();
            base.any_of.clear();
            base.one_of.clear();
            let mut hint = Node::Schema(Box::new(base));
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
                self.validated(&Node::Schema(Box::new(structural)), depth + 1)
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
    fn compile(&self, value: &Value, dialect: Dialect) -> Result<Node> {
        Node::compile(value, dialect, &Compilation { names: self.names, locales: self.locales })
    }
    fn invoke(
        &mut self,
        kind: ProviderKind,
        name: &str,
        locale: &str,
        options: Map<String, Value>,
        node: &SchemaNode,
    ) -> Result<Value> {
        let context = ProviderContext {
            kind,
            name: name.to_owned(),
            seed: self.random.next_u64(),
            locale: locale.to_owned(),
            schema: node.contract.clone(),
            options,
        };
        let value = (self.provider)(&context)
            .map_err(|error| Error::Provider(format!("Provider {name} failed: {error}")))?;
        crate::limits::validate_numbers(&value, 0).map_err(|error| {
            Error::Provider(format!("Provider {name} returned unsupported numeric data: {error}"))
        })?;
        Ok(value)
    }
    fn string(&mut self, node: &SchemaNode) -> Result<Value> {
        const FORMATS: &[&str] = &[
            "email",
            "idn-email",
            "hostname",
            "idn-hostname",
            "uri",
            "uri-reference",
            "iri",
            "iri-reference",
            "uri-template",
            "uuid",
            "date",
            "date-time",
            "time",
            "duration",
            "ipv4",
            "ipv6",
            "json-pointer",
            "relative-json-pointer",
            "regex",
            "color",
        ];
        if let Some(format) = &node.data.format
            && FORMATS.contains(&format.as_str())
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
        let closed = (node.contract.get("items").is_some_and(Value::is_array)
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
        let length = scalars::size(
            node.data.min_items.max(contains_minimum),
            maximum,
            5,
            self.profile == Profile::Boundary,
            &mut self.random,
        )?;
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
                    self.compile(&json!({"not": contains.contract()}), node.dialect)?
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
            .filter(|(name, child)| !selected.contains(*name) && !child.is_false())
            .map(|(name, _)| name.clone())
            .collect();
        let extra_allowed = !node.additional.is_false()
            && (!node.unevaluated_properties.as_ref().is_some_and(Node::is_false)
                || node.applied.additional);
        let minimum = node.data.min_properties.max(selected.len());
        let default = minimum.max(
            node.properties.len() + if extra_allowed || !node.patterns.is_empty() { 2 } else { 0 },
        );
        let wanted = scalars::size(
            minimum,
            node.data.max_properties,
            default,
            self.profile == Profile::Boundary,
            &mut self.random,
        )?;
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
            let mut hint = Node::Schema(Box::new(base));
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
            for index in 0..self.max_attempts * 100 {
                if values.len() >= wanted {
                    break;
                }
                self.spend(depth)?;
                let name = self.extra_name(node, index, depth)?;
                if values.contains_key(&name) || node.properties.contains_key(&name) {
                    continue;
                }
                let child = Self::property(node, &name)?;
                if !child.is_false() {
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
        Ok(format!("extra_{index}"))
    }
}
