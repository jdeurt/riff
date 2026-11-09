//! Compile untrusted schema data into concrete generation constraints.
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use bigdecimal::BigDecimal;

use jsonschema::{Draft, Validator};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::metadata::BUILTINS;
use crate::schema::{Dialect, child_locations, prepare};
use crate::{Error, Metadata, Result, Source, Strategy};

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Types {
    Single(Kind),
    Multiple(Vec<Kind>),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Keywords {
    #[serde(rename = "type")]
    pub kind: Option<Types>,
    #[serde(rename = "enum")]
    pub enumeration: Option<Vec<Value>>,
    #[serde(rename = "const")]
    pub constant: Value,
    pub required: Vec<String>,
    #[serde(deserialize_with = "decimal_constraint")]
    pub minimum: Option<BigDecimal>,
    #[serde(deserialize_with = "decimal_constraint")]
    pub maximum: Option<BigDecimal>,
    #[serde(deserialize_with = "decimal_constraint")]
    pub exclusive_minimum: Option<BigDecimal>,
    #[serde(deserialize_with = "decimal_constraint")]
    pub exclusive_maximum: Option<BigDecimal>,
    #[serde(deserialize_with = "decimal_constraint")]
    pub multiple_of: Option<BigDecimal>,
    pub min_length: usize,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
    pub format: Option<String>,
    pub min_items: usize,
    pub max_items: Option<usize>,
    pub unique_items: bool,
    pub min_properties: usize,
    pub max_properties: Option<usize>,
    pub min_contains: Option<usize>,
    pub max_contains: Option<usize>,
    pub dependent_required: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug)]
pub enum Node {
    Boolean(bool),
    Schema(Box<SchemaNode>),
}

#[derive(Clone, Debug)]
pub struct SchemaNode {
    pub contract: Value,
    pub dialect: Dialect,
    pub data: Keywords,
    pub metadata: Metadata,
    pub validator: Arc<Validator>,
    pub constant_present: bool,
    pub properties: BTreeMap<String, Node>,
    pub patterns: Vec<Pattern>,
    pub additional: Node,
    pub items: Node,
    pub tuple: Vec<Node>,
    pub additional_items: Node,
    pub all_of: Vec<Node>,
    pub any_of: Vec<Node>,
    pub one_of: Vec<Node>,
    pub contains: Option<Node>,
    pub names: Option<Node>,
    pub condition: Option<Node>,
    pub then_schema: Option<Node>,
    pub else_schema: Option<Node>,
    pub dependencies: BTreeMap<String, Node>,
    pub required_dependencies: BTreeMap<String, Vec<String>>,
    pub unevaluated_properties: Option<Node>,
    pub unevaluated_items: Option<Node>,
    pub applied: AppliedKeywords,
}

#[derive(Clone, Debug)]
pub struct AppliedKeywords {
    pub additional: bool,
    pub items: bool,
    pub additional_items: bool,
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub source: String,
    pub regex: fancy_regex::Regex,
    pub node: Node,
}

pub struct Compilation<'a> {
    pub names: &'a BTreeSet<String>,
    pub locales: &'a BTreeSet<String>,
}

pub fn validator(schema: &Value, dialect: Dialect) -> Result<Arc<Validator>> {
    let draft = match dialect {
        Dialect::Draft7 => Draft::Draft7,
        Dialect::Draft2019 => Draft::Draft201909,
        Dialect::Draft2020 => Draft::Draft202012,
    };
    let mut contract = schema.clone();
    remove_dialect_annotations(&mut contract, dialect)?;
    jsonschema::options()
        .with_draft(draft)
        .should_validate_formats(true)
        .should_ignore_unknown_formats(true)
        .build(&contract)
        .map(Arc::new)
        .map_err(|error| Error::Schema(format!("Invalid JSON Schema: {error}")))
}

fn remove_dialect_annotations(schema: &mut Value, dialect: Dialect) -> Result<()> {
    if let Some(Value::String(uri)) = schema.get("$schema")
        && Dialect::select(uri)? != dialect
    {
        return Err(Error::Schema("Nested dialect switching is unsupported.".to_owned()));
    }
    let locations = child_locations(schema);
    for location in locations {
        if let Some(child) = schema.pointer_mut(&location) {
            remove_dialect_annotations(child, dialect)?;
        }
    }
    if let Some(object) = schema.as_object_mut() {
        let _ = object.remove("$schema");
        let inactive: &[&str] = match dialect {
            Dialect::Draft7 => &[
                "dependentRequired",
                "dependentSchemas",
                "unevaluatedProperties",
                "unevaluatedItems",
                "prefixItems",
                "minContains",
                "maxContains",
            ],
            Dialect::Draft2019 => &["dependencies", "prefixItems"],
            Dialect::Draft2020 => &["dependencies", "additionalItems"],
        };
        for keyword in inactive {
            let _ = object.remove(*keyword);
        }
    }
    Ok(())
}

impl Node {
    pub fn compile(schema: &Value, inherited: Dialect, context: &Compilation<'_>) -> Result<Self> {
        validate_declarations(schema, inherited, context)?;
        Self::build(schema, inherited)
    }

    fn build(schema: &Value, inherited: Dialect) -> Result<Self> {
        if let Value::Bool(value) = schema {
            return Ok(Self::Boolean(*value));
        }
        let object = schema
            .as_object()
            .ok_or_else(|| Error::Schema("Expected a schema object.".to_owned()))?;
        let dialect = match object.get("$schema") {
            Some(Value::String(uri)) => Dialect::select(uri)?,
            _ => inherited,
        };
        let data: Keywords = serde_json::from_value(schema.clone())?;
        let metadata = Metadata::parse(object.get("x-fixture"))?;
        let checker = validator(schema, dialect)?;
        let compile = |input: &Value| Self::build(input, dialect);
        let optional = |name: &str| object.get(name).map(compile).transpose();
        let properties = compile_map(object.get("properties"), &compile)?;
        let pattern_nodes = compile_map(object.get("patternProperties"), &compile)?;
        let patterns = pattern_nodes
            .into_iter()
            .map(|(source, node)| {
                let regex = fancy_regex::Regex::new(&source)
                    .map_err(|error| Error::Schema(error.to_string()))?;
                Ok(Pattern { source, regex, node })
            })
            .collect::<Result<_>>()?;
        let additional = optional("additionalProperties")?.unwrap_or(Self::Boolean(true));
        let raw_items = object.get("items");
        let items_array = raw_items.and_then(Value::as_array);
        let (items, tuple) = if let Some(values) = items_array {
            (Self::Boolean(true), values.iter().map(compile).collect::<Result<_>>()?)
        } else {
            (
                optional("items")?.unwrap_or(Self::Boolean(true)),
                if dialect == Dialect::Draft2020 {
                    compile_list(object.get("prefixItems"), &compile)?
                } else {
                    Vec::new()
                },
            )
        };
        let dependencies = compile_dependencies(object, &data, dialect, &compile)?;
        Ok(Self::Schema(Box::new(SchemaNode {
            contract: schema.clone(),
            dialect,
            data,
            metadata,
            validator: checker,
            constant_present: object.contains_key("const"),
            properties,
            patterns,
            additional,
            items,
            tuple,
            additional_items: optional("additionalItems")?.unwrap_or(Self::Boolean(true)),
            all_of: compile_list(object.get("allOf"), &compile)?,
            any_of: compile_list(object.get("anyOf"), &compile)?,
            one_of: compile_list(object.get("oneOf"), &compile)?,
            contains: optional("contains")?,
            names: optional("propertyNames")?,
            condition: optional("if")?,
            then_schema: optional("then")?,
            else_schema: optional("else")?,
            dependencies: dependencies.schemas,
            required_dependencies: dependencies.required,
            unevaluated_properties: if dialect == Dialect::Draft7 {
                None
            } else {
                optional("unevaluatedProperties")?
            },
            unevaluated_items: if dialect == Dialect::Draft7 {
                None
            } else {
                optional("unevaluatedItems")?
            },
            applied: AppliedKeywords {
                additional: object.contains_key("additionalProperties"),
                items: object.contains_key("items") && items_array.is_none(),
                additional_items: object.contains_key("additionalItems"),
            },
        })))
    }
    pub fn valid(&self, value: &Value) -> bool {
        match self {
            Self::Boolean(valid) => *valid,
            Self::Schema(node) => node.validator.is_valid(value),
        }
    }
    pub fn contract(&self) -> Value {
        match self {
            Self::Boolean(value) => Value::Bool(*value),
            Self::Schema(node) => node.contract.clone(),
        }
    }
    pub const fn is_false(&self) -> bool {
        matches!(self, Self::Boolean(false))
    }
}

fn validate_declarations(
    schema: &Value,
    inherited: Dialect,
    context: &Compilation<'_>,
) -> Result<()> {
    let Some(object) = schema.as_object() else { return Ok(()) };
    let dialect = match object.get("$schema") {
        Some(Value::String(uri)) => Dialect::select(uri)?,
        _ => inherited,
    };
    let data: Keywords = serde_json::from_value(schema.clone())?;
    let metadata = Metadata::parse(object.get("x-fixture"))?;
    for source in std::iter::once(&metadata.source).chain(metadata.failure.iter()) {
        source.validate_names(context.names)?;
        validate_source(source, &data, context)?;
        if let Some(strategy) = &source.strategy {
            validate_predicates(strategy, context, dialect)?;
        }
    }
    if let Some(values) = &metadata.source.examples
        && values
            .iter()
            .any(|value| validator(schema, dialect).is_ok_and(|checker| !checker.is_valid(value)))
    {
        return Err(Error::Schema(
            "Positive shorthand examples must satisfy the annotated schema.".to_owned(),
        ));
    }

    for pointer in child_locations(schema) {
        if let Some(child) = schema.pointer(&pointer) {
            validate_declarations(child, dialect, context)?;
        }
    }
    Ok(())
}

struct Dependencies {
    schemas: BTreeMap<String, Node>,
    required: BTreeMap<String, Vec<String>>,
}

fn compile_dependencies(
    object: &serde_json::Map<String, Value>,
    data: &Keywords,
    dialect: Dialect,
    compile: &impl Fn(&Value) -> Result<Node>,
) -> Result<Dependencies> {
    let mut dependencies = BTreeMap::new();
    let mut required_dependencies = BTreeMap::new();
    if dialect == Dialect::Draft7 {
        if let Some(Value::Object(entries)) = object.get("dependencies") {
            for (name, value) in entries {
                if value.is_array() {
                    let _ = required_dependencies
                        .insert(name.clone(), serde_json::from_value(value.clone())?);
                } else {
                    let _ = dependencies.insert(name.clone(), compile(value)?);
                }
            }
        }
    } else {
        dependencies = compile_map(object.get("dependentSchemas"), &compile)?;
        required_dependencies.clone_from(&data.dependent_required);
    }
    Ok(Dependencies { schemas: dependencies, required: required_dependencies })
}

fn compile_map(
    input: Option<&Value>,
    compile: &impl Fn(&Value) -> Result<Node>,
) -> Result<BTreeMap<String, Node>> {
    match input {
        Some(Value::Object(entries)) => {
            entries.iter().map(|(name, child)| Ok((name.clone(), compile(child)?))).collect()
        }
        None => Ok(BTreeMap::new()),
        _ => Err(Error::Schema("Schema map must be an object.".to_owned())),
    }
}

fn compile_list(
    input: Option<&Value>,
    compile: &impl Fn(&Value) -> Result<Node>,
) -> Result<Vec<Node>> {
    match input {
        Some(Value::Array(values)) => values.iter().map(compile).collect(),
        None => Ok(Vec::new()),
        _ => Err(Error::Schema("Schema alternatives must be an array.".to_owned())),
    }
}

fn validate_source(source: &Source, data: &Keywords, context: &Compilation<'_>) -> Result<()> {
    if let Some(name) = &source.provider {
        validate_builtin(name, source.locale.as_deref().unwrap_or("en-US"), data, context)?;
    }
    if let Some(strategy) = &source.strategy {
        validate_strategy_provider(strategy, data, context)?;
    }
    Ok(())
}

fn validate_builtin(
    name: &str,
    locale: &str,
    data: &Keywords,
    context: &Compilation<'_>,
) -> Result<()> {
    if context.names.contains(name) {
        return Ok(());
    }
    let string = match &data.kind {
        None => true,
        Some(Types::Single(kind)) => *kind == Kind::String,
        Some(Types::Multiple(kinds)) => kinds.contains(&Kind::String),
    };
    if BUILTINS.contains(&name) && !string {
        return Err(Error::Schema(
            "Default semantic providers require a string schema.".to_owned(),
        ));
    }
    if !context.locales.contains(locale) {
        return Err(Error::Schema(format!("Unsupported locale: {locale}")));
    }
    Ok(())
}

fn validate_strategy_provider(
    strategy: &Strategy,
    data: &Keywords,
    context: &Compilation<'_>,
) -> Result<()> {
    match strategy {
        Strategy::Provider { name, locale, .. } => validate_builtin(name, locale, data, context)?,
        Strategy::Choice { strategies } => {
            for child in strategies {
                validate_strategy_provider(child, data, context)?;
            }
        }
        Strategy::Filter { strategy, .. } => validate_strategy_provider(strategy, data, context)?,
        _ => {}
    }
    Ok(())
}

fn validate_predicates(
    strategy: &Strategy,
    context: &Compilation<'_>,
    dialect: Dialect,
) -> Result<()> {
    match strategy {
        Strategy::Filter { schema, strategy } => {
            let normalized = prepare(schema, None, None)?;
            let _ = Node::compile(&normalized, dialect, context)?;
            validate_predicates(strategy, context, dialect)?;
        }
        Strategy::Choice { strategies } => {
            for child in strategies {
                validate_predicates(child, context, dialect)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl SchemaNode {
    pub fn item(&self, index: usize) -> Node {
        if let Some(child) = self.tuple.get(index) {
            return child.clone();
        }
        if self.contract.get("items").is_some_and(Value::is_array) {
            if !self.applied.additional_items
                && !self.applied.items
                && let Some(child) = &self.unevaluated_items
            {
                return child.clone();
            }
            return self.additional_items.clone();
        }
        if !self.applied.items
            && let Some(child) = &self.unevaluated_items
        {
            return if child.is_false() {
                self.contains.clone().unwrap_or_else(|| child.clone())
            } else {
                child.clone()
            };
        }
        self.items.clone()
    }
    pub fn matching_patterns(&self, name: &str) -> Result<Vec<&Node>> {
        self.patterns
            .iter()
            .filter_map(|pattern| match pattern.regex.is_match(name) {
                Ok(true) => Some(Ok(&pattern.node)),
                Ok(false) => None,
                Err(error) => Some(Err(Error::Generation(error.to_string()))),
            })
            .collect()
    }
}

pub fn intersect(left: &Node, right: &Node) -> Result<Node> {
    if left.is_false() || right.is_false() {
        return Ok(Node::Boolean(false));
    }
    let (Node::Schema(left), Node::Schema(right)) = (left, right) else {
        return Ok(if matches!(left, Node::Boolean(true)) { right.clone() } else { left.clone() });
    };
    let mut result = left.clone();
    result.contract = json!({"allOf": [left.contract, right.contract]});
    result.validator = validator(&result.contract, left.dialect)?;
    if right.contract.get("x-fixture").is_some() {
        result.metadata.clone_from(&right.metadata);
    }
    if right.data.kind.is_some() {
        result.data.kind.clone_from(&right.data.kind);
    }
    if right.data.enumeration.is_some() {
        result.data.enumeration.clone_from(&right.data.enumeration);
    }
    if right.constant_present {
        result.constant_present = true;
        result.data.constant.clone_from(&right.data.constant);
    }
    for name in &right.data.required {
        if !result.data.required.contains(name) {
            result.data.required.push(name.clone());
        }
    }
    for (name, child) in &right.properties {
        let merged = intersect(result.properties.get(name).unwrap_or(&Node::Boolean(true)), child)?;
        let _ = result.properties.insert(name.clone(), merged);
    }
    result.additional = intersect(&left.additional, &right.additional)?;
    result.items = intersect(&left.items, &right.items)?;
    let length = left.tuple.len().max(right.tuple.len());
    result.tuple = (0..length)
        .map(|index| intersect(&left.item(index), &right.item(index)))
        .collect::<Result<_>>()?;
    result.additional_items = intersect(&left.additional_items, &right.additional_items)?;
    if right.contains.is_some() {
        result.contains.clone_from(&right.contains);
    }
    if !right.all_of.is_empty() {
        result.all_of.clone_from(&right.all_of);
    }
    if !right.any_of.is_empty() {
        result.any_of.clone_from(&right.any_of);
    }
    if !right.one_of.is_empty() {
        result.one_of.clone_from(&right.one_of);
    }
    if right.condition.is_some() {
        result.condition.clone_from(&right.condition);
        result.then_schema.clone_from(&right.then_schema);
        result.else_schema.clone_from(&right.else_schema);
    }
    result.patterns.extend(right.patterns.clone());
    result.dependencies.extend(right.dependencies.clone());
    for (name, keys) in &right.required_dependencies {
        let current = result.required_dependencies.entry(name.clone()).or_default();
        for key in keys {
            if !current.contains(key) {
                current.push(key.clone());
            }
        }
    }
    if let Some(names) = &right.names {
        result.names =
            Some(intersect(result.names.as_ref().unwrap_or(&Node::Boolean(true)), names)?);
    }
    if right.unevaluated_properties.is_some() {
        result.unevaluated_properties.clone_from(&right.unevaluated_properties);
    }
    if right.unevaluated_items.is_some() {
        result.unevaluated_items.clone_from(&right.unevaluated_items);
    }
    result.applied.additional |= right.applied.additional;
    result.applied.items |= right.applied.items;
    result.applied.additional_items |= right.applied.additional_items;
    merge_limits(&mut result.data, &right.data);
    Ok(Node::Schema(result))
}

fn merge_limits(result: &mut Keywords, right: &Keywords) {
    result.minimum = lower(result.minimum.as_ref(), right.minimum.as_ref());
    result.maximum = upper(result.maximum.as_ref(), right.maximum.as_ref());
    result.exclusive_minimum =
        lower(result.exclusive_minimum.as_ref(), right.exclusive_minimum.as_ref());
    result.exclusive_maximum =
        upper(result.exclusive_maximum.as_ref(), right.exclusive_maximum.as_ref());
    result.min_length = result.min_length.max(right.min_length);
    result.max_length = upper_size(result.max_length, right.max_length);
    result.min_items = result.min_items.max(right.min_items);
    result.max_items = upper_size(result.max_items, right.max_items);
    result.min_properties = result.min_properties.max(right.min_properties);
    result.max_properties = upper_size(result.max_properties, right.max_properties);
    if right.multiple_of.is_some() {
        result.multiple_of.clone_from(&right.multiple_of);
    }
    if right.pattern.is_some() {
        result.pattern.clone_from(&right.pattern);
    }
    if right.format.is_some() {
        result.format.clone_from(&right.format);
    }
    if right.min_contains.is_some() {
        result.min_contains = right.min_contains;
    }
    result.max_contains = upper_size(result.max_contains, right.max_contains);
    result.unique_items |= right.unique_items;
}

fn lower(left: Option<&BigDecimal>, right: Option<&BigDecimal>) -> Option<BigDecimal> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right).clone()),
        _ => left.or(right).cloned(),
    }
}
fn upper(left: Option<&BigDecimal>, right: Option<&BigDecimal>) -> Option<BigDecimal> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right).clone()),
        _ => left.or(right).cloned(),
    }
}
fn upper_size(left: Option<usize>, right: Option<usize>) -> Option<usize> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        _ => left.or(right),
    }
}

fn decimal_constraint<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<BigDecimal>, D::Error> {
    Option::<serde_json::Number>::deserialize(deserializer)?
        .map(|number| BigDecimal::from_str(&number.to_string()).map_err(serde::de::Error::custom))
        .transpose()
}
