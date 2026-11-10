//! Compile untrusted schema data into concrete generation constraints.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::str::FromStr;
use std::sync::Arc;

use bigdecimal::BigDecimal;

use jsonschema::{ValidationError, Validator};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::contracts::DeclaredContract;
use crate::location::SchemaLocation;
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
    Schema(Arc<SchemaNode>),
}

#[derive(Clone, Debug)]
pub struct SchemaNode {
    pub sampling_schema: Value,
    pub declared: Arc<DeclaredContract>,
    pub dialect: Dialect,
    pub data: Keywords,
    pub metadata: Metadata,
    pub validator: Checker,
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
}

#[derive(Clone, Debug)]
pub struct Checker {
    validators: Vec<Arc<Validator>>,
}

impl Checker {
    pub fn is_valid(&self, value: &Value) -> bool {
        self.validators.iter().all(|validator| validator.is_valid(value))
    }

    pub fn iter_errors<'a>(
        &'a self,
        value: &'a Value,
    ) -> impl Iterator<Item = ValidationError<'a>> + 'a {
        self.validators.iter().flat_map(move |validator| validator.iter_errors(value))
    }

    fn intersect(&self, right: &Self) -> Self {
        let mut validators = self.validators.clone();
        for validator in &right.validators {
            if !validators.iter().any(|existing| Arc::ptr_eq(existing, validator)) {
                validators.push(validator.clone());
            }
        }
        Self { validators }
    }
}

const VALIDATION_DOCUMENT: &str = "https://riff.invalid/validation-document";

pub struct Validators {
    registry: jsonschema::Registry<'static>,
    cache: RefCell<BTreeMap<SchemaLocation, Arc<Validator>>>,
}

impl Validators {
    fn new(schema: &Value) -> Result<Self> {
        let root = jsonschema::options()
            .should_validate_formats(true)
            .should_ignore_unknown_formats(true)
            .build(schema)
            .map_err(|error| Error::Schema(error.to_string()))?;
        let registry = jsonschema::Registry::new()
            .retrieve_known_draft_meta_schemas(false)
            .add(VALIDATION_DOCUMENT, schema.clone())
            .and_then(jsonschema::RegistryBuilder::prepare)
            .map_err(|error| Error::Schema(error.to_string()))?;
        Ok(Self {
            registry,
            cache: RefCell::new(BTreeMap::from([(SchemaLocation::root(), Arc::new(root))])),
        })
    }

    pub fn get(&self, location: &SchemaLocation) -> Result<Arc<Validator>> {
        if let Some(validator) = self.cache.borrow().get(location) {
            return Ok(validator.clone());
        }
        let target = location.reference(VALIDATION_DOCUMENT);
        let schema = json!({"$ref":target});
        let validator = jsonschema::options()
            .with_registry(&self.registry)
            .should_validate_formats(true)
            .should_ignore_unknown_formats(true)
            .build(&schema)
            .map_err(|error| Error::Schema(error.to_string()))?;
        let validator = Arc::new(validator);
        let _ = self.cache.borrow_mut().insert(location.clone(), validator.clone());
        Ok(validator)
    }
}

struct Builder<'a> {
    document: &'a Value,
    sources: &'a BTreeMap<String, SchemaLocation>,
    validators: &'a Validators,
    paths: HashMap<*const Value, String>,
    resources: Arc<serde_json::Map<String, Value>>,
    cache: RefCell<BTreeMap<SchemaLocation, HashMap<Value, Node>>>,
}

impl Node {
    pub fn compile(schema: &Value, inherited: Dialect, context: &Compilation<'_>) -> Result<Self> {
        let validators = Validators::new(schema)?;
        validate_declarations(schema, inherited, context, &SchemaLocation::root(), &validators)?;
        let plan = crate::references::sampling_schema(schema, &validators)?;
        let mut builder = Builder {
            document: schema,
            sources: &plan.sources,
            validators: &validators,
            paths: HashMap::new(),
            resources: Arc::new(
                schema.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default(),
            ),
            cache: RefCell::new(BTreeMap::new()),
        };
        builder.index(&plan.schema, "#");
        builder.build(&plan.schema, "#", inherited)
    }
}

impl Builder<'_> {
    fn index(&mut self, input: &Value, path: &str) {
        let _ = self.paths.insert(std::ptr::from_ref(input), path.to_owned());
        for location in child_locations(input) {
            if let Some(child) = input.pointer(&location) {
                self.index(child, &format!("{path}{location}"));
            }
        }
    }

    fn build(&self, schema: &Value, path: &str, inherited: Dialect) -> Result<Node> {
        let source = self
            .sources
            .get(path)
            .ok_or_else(|| Error::Schema(format!("Missing original schema for {path}")))?;
        if path != "#"
            && let Some(node) =
                self.cache.borrow().get(source).and_then(|nodes| nodes.get(schema)).cloned()
        {
            return Ok(node);
        }
        let original = source.resolve(self.document)?;
        let node = self.build_node(schema, original, source, inherited)?;
        let _ = self
            .cache
            .borrow_mut()
            .entry(source.clone())
            .or_default()
            .insert(schema.clone(), node.clone());
        Ok(node)
    }

    fn build_node(
        &self,
        schema: &Value,
        original: &Value,
        source: &SchemaLocation,
        inherited: Dialect,
    ) -> Result<Node> {
        if let Value::Bool(value) = schema
            && original.is_boolean()
        {
            return Ok(Node::Boolean(*value));
        }
        let empty = json!({"enum":[]});
        let hint = if schema.is_boolean() { &empty } else { schema };
        let object = hint
            .as_object()
            .ok_or_else(|| Error::Schema("Expected a sampling schema object.".to_owned()))?;
        let dialect = inherited;
        let metadata = Metadata::parse(original.get("x-fixture"))?;
        let declared = metadata.source.provider.is_some()
            || metadata.source.examples.is_some()
            || metadata.source.strategy.is_some();
        let constraints = if declared { original } else { hint };
        let data: Keywords = crate::documents::decode(constraints)?;
        let checker = Checker { validators: vec![self.validators.get(source)?] };
        let compile = |input: &Value| {
            let pointer = std::ptr::from_ref(input);
            let child_path = self
                .paths
                .get(&pointer)
                .ok_or_else(|| Error::Schema("Missing sampling node location.".to_owned()))?;
            self.build(input, child_path, dialect)
        };
        let optional = |name: &str| object.get(name).map(compile).transpose();
        let properties = compile_map(object.get("properties"), &compile)?;
        let pattern_nodes = compile_map(object.get("patternProperties"), &compile)?;
        let patterns = pattern_nodes
            .into_iter()
            .map(|(source, node)| {
                let translated = jsonschema_regex::to_rust_regex(&source).map_err(|()| {
                    Error::Schema("Invalid ECMAScript property pattern.".to_owned())
                })?;
                let regex = fancy_regex::Regex::new(&translated)
                    .map_err(|error| Error::Schema(error.to_string()))?;
                Ok(Pattern { source, regex, node })
            })
            .collect::<Result<_>>()?;
        let additional = optional("additionalProperties")?.unwrap_or(Node::Boolean(true));
        let raw_items = object.get("items");
        let items_array = raw_items.and_then(Value::as_array);
        let (items, tuple) = if let Some(values) = items_array {
            (Node::Boolean(true), values.iter().map(compile).collect::<Result<_>>()?)
        } else {
            (
                optional("items")?.unwrap_or(Node::Boolean(true)),
                if dialect == Dialect::Draft2020 {
                    compile_list(object.get("prefixItems"), &compile)?
                } else {
                    Vec::new()
                },
            )
        };
        let dependencies = compile_dependencies(object, &data, dialect, &compile)?;
        Ok(Node::Schema(Arc::new(SchemaNode {
            sampling_schema: schema.clone(),
            declared: Arc::new(DeclaredContract::Schema {
                value: original.clone(),
                resources: self.resources.clone(),
            }),
            dialect,
            data,
            metadata,
            validator: checker,
            constant_present: constraints.get("const").is_some(),
            properties,
            patterns,
            additional,
            items,
            tuple,
            additional_items: optional("additionalItems")?.unwrap_or(Node::Boolean(true)),
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
}

impl Node {
    pub fn valid(&self, value: &Value) -> bool {
        match self {
            Self::Boolean(valid) => *valid,
            Self::Schema(node) => node.validator.is_valid(value),
        }
    }
    pub fn sampling_schema(&self) -> Value {
        match self {
            Self::Boolean(value) => Value::Bool(*value),
            Self::Schema(node) => node.sampling_schema.clone(),
        }
    }
    pub fn can_sample(&self) -> bool {
        match self {
            Self::Boolean(value) => *value,
            Self::Schema(node) => {
                node.metadata.source.provider.is_some()
                    || node.metadata.source.examples.is_some()
                    || node.metadata.source.strategy.is_some()
                    || (node.constant_present && node.validator.is_valid(&node.data.constant))
                    || node.data.enumeration.as_ref().is_some_and(|values| {
                        values.iter().any(|value| node.validator.is_valid(value))
                    })
                    || !node.impossible()
            }
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
    path: &SchemaLocation,
    validators: &Validators,
) -> Result<()> {
    let Some(object) = schema.as_object() else { return Ok(()) };
    let dialect = match object.get("$schema") {
        Some(Value::String(uri)) => Dialect::select(uri)?,
        _ => inherited,
    };
    let data: Keywords = crate::documents::decode(schema)?;
    let metadata = Metadata::parse(object.get("x-fixture"))?;
    for source in std::iter::once(&metadata.source).chain(metadata.failure.iter()) {
        source.validate_names(context.names)?;
        validate_source(source, &data, context)?;
        if let Some(strategy) = &source.strategy {
            validate_predicates(strategy, context, dialect)?;
        }
    }
    if let Some(values) = &metadata.source.examples {
        let checker = validators.get(path)?;
        if values.iter().any(|value| !checker.is_valid(value)) {
            return Err(Error::Schema(
                "Positive shorthand examples must satisfy the annotated schema.".to_owned(),
            ));
        }
    }

    for pointer in child_locations(schema) {
        if let Some(child) = schema.pointer(&pointer) {
            validate_declarations(child, dialect, context, &path.child(&pointer), validators)?;
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
    if !crate::providers::LOCALES.contains(&locale) {
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
    fn impossible(&self) -> bool {
        if self.data.enumeration.as_ref().is_some_and(Vec::is_empty)
            || self.all_of.iter().any(|node| !node.can_sample())
            || (!self.any_of.is_empty() && self.any_of.iter().all(|node| !node.can_sample()))
            || (!self.one_of.is_empty() && self.one_of.iter().all(|node| !node.can_sample()))
        {
            return true;
        }
        match self.data.kind {
            Some(Types::Single(Kind::Array)) => {
                self.data.min_items > self.data.max_items.unwrap_or(usize::MAX)
                    || (self.items.is_false()
                        && self.applied.items
                        && self.data.min_items > self.tuple.len())
            }
            Some(Types::Single(Kind::Object)) => {
                self.data.min_properties > self.data.max_properties.unwrap_or(usize::MAX)
                    || self.data.required.iter().any(|name| {
                        self.properties.get(name).is_some_and(|node| !node.can_sample())
                    })
            }
            _ => false,
        }
    }

    pub fn item(&self, index: usize) -> Node {
        if let Some(child) = self.tuple.get(index) {
            return child.clone();
        }
        if self.sampling_schema.get("items").is_some_and(Value::is_array) {
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
    let mut result = left.as_ref().clone();
    result.sampling_schema = json!({"allOf": [left.sampling_schema, right.sampling_schema]});
    result.declared =
        Arc::new(DeclaredContract::Intersection(left.declared.clone(), right.declared.clone()));
    result.validator = left.validator.intersect(&right.validator);
    if right.sampling_schema.get("x-fixture").is_some() {
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
    Ok(Node::Schema(Arc::new(result)))
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
