//! Offline reference graphs and finite generation plans for recursive contracts.
use std::collections::{BTreeMap, BTreeSet};

use jsonschema::{Draft, Registry};
use referencing::{Resolver, ResourceRef};
use serde_json::{Map, Value, json};

use crate::location::SchemaLocation;
use crate::schema::{CHILDREN, Dialect, KEYWORDS, LISTS, MAPS};
use crate::{Error, Result};

const DOCUMENT_URI: &str = "https://riff.invalid/input-document";
const MAX_SCHEMA_NODES: usize = 16_384;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Reference {
    base: String,
    link: String,
    scope: Vec<(String, *const Value)>,
}

#[derive(Clone, Copy)]
enum ScopePosition {
    Inline,
    Target,
}

struct Bundler<'a> {
    registry: &'a Registry<'a>,
    original: &'a Value,
    definitions: Map<String, Value>,
    references: BTreeMap<Reference, String>,
    remaining: usize,
    openapi30: bool,
    anchors: BTreeSet<String>,
    document_dialect: Option<Dialect>,
}

pub fn bundle(schema: &Value, document: &Value, openapi30: bool) -> Result<Value> {
    let document_dialect = document
        .get("openapi")
        .and_then(Value::as_str)
        .map(|_| {
            document
                .get("jsonSchemaDialect")
                .and_then(Value::as_str)
                .map(Dialect::select)
                .transpose()
                .map(Option::unwrap_or_default)
        })
        .transpose()?;
    let indexed =
        crate::openapi::indexed_document(schema, document, document_dialect, DOCUMENT_URI)?;
    let registry = Registry::new()
        .retrieve_known_draft_meta_schemas(false)
        .add(DOCUMENT_URI, &indexed.document)
        .and_then(|builder| {
            builder.extend(indexed.resources.iter().map(|(uri, resource)| (uri, resource)))
        })
        .and_then(jsonschema::RegistryBuilder::prepare)
        .map_err(|error| Error::Schema(error.to_string()))?;
    let base = jsonschema::uri::from_str(DOCUMENT_URI)
        .map_err(|error| Error::Schema(error.to_string()))?;
    let resolver = registry.resolver(base);
    let mut bundler = Bundler {
        registry: &registry,
        original: document,
        definitions: Map::new(),
        references: BTreeMap::new(),
        remaining: MAX_SCHEMA_NODES,
        openapi30,
        anchors: dynamic_anchors(&indexed.document),
        document_dialect,
    };
    let mut result =
        bundler.schema(schema, &resolver, Dialect::default(), 0, ScopePosition::Inline)?;
    if let Some(object) = result.as_object_mut() {
        if !bundler.definitions.is_empty() {
            let _ = object.insert("$defs".to_owned(), Value::Object(bundler.definitions));
        }
        let _ = object
            .insert("$schema".to_owned(), json!("https://json-schema.org/draft/2020-12/schema"));
    }
    Ok(result)
}

impl Bundler<'_> {
    fn schema(
        &mut self,
        input: &Value,
        resolver: &Resolver<'_>,
        inherited: Dialect,
        depth: usize,
        position: ScopePosition,
    ) -> Result<Value> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            Error::Schema("Schema reference graph node budget exhausted.".to_owned())
        })?;
        if depth > 128 {
            return Err(Error::Schema("Schema reference graph depth exceeded.".to_owned()));
        }
        if input.is_boolean() {
            return Ok(input.clone());
        }
        let object = input
            .as_object()
            .ok_or_else(|| Error::Schema("Each schema must be an object or boolean.".to_owned()))?;
        let dialect = object
            .get("$schema")
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| Error::Schema("$schema must be a URI.".to_owned()))
                    .and_then(Dialect::select)
            })
            .transpose()?
            .unwrap_or(inherited);
        let draft = match dialect {
            Dialect::Draft4 => Draft::Draft4,
            Dialect::Draft7 => Draft::Draft7,
            Dialect::Draft2019 => Draft::Draft201909,
            Dialect::Draft2020 => Draft::Draft202012,
        };
        let scoped = if matches!(position, ScopePosition::Target) {
            resolver.clone()
        } else {
            resolver
                .in_subresource(ResourceRef::new(input, draft))
                .map_err(|error| Error::Schema(error.to_string()))?
        };
        let mut result = object.clone();
        match dialect {
            Dialect::Draft4 | Dialect::Draft7 => {
                let _ = result.remove("$dynamicRef");
                let _ = result.remove("$recursiveRef");
            }
            Dialect::Draft2019 => {
                let _ = result.remove("$dynamicRef");
            }
            Dialect::Draft2020 => {
                let _ = result.remove("$recursiveRef");
            }
        }
        let mut targets = Vec::new();
        for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            if let Some(reference) = result.remove(keyword) {
                let reference = reference
                    .as_str()
                    .ok_or_else(|| Error::Schema(format!("{keyword} must be a URI.")))?;
                let target = self.reference(reference, keyword, &scoped, depth)?;
                targets.push(target);
                if matches!(dialect, Dialect::Draft4 | Dialect::Draft7) {
                    let metadata = result.get("x-fixture").cloned();
                    result.clear();
                    if let Some(metadata) = metadata {
                        let _ = result.insert("x-fixture".to_owned(), metadata);
                    }
                    break;
                }
            }
        }
        for keyword in [
            "$id",
            "id",
            "$schema",
            "$defs",
            "definitions",
            "$anchor",
            "$dynamicAnchor",
            "$recursiveAnchor",
            "$vocabulary",
        ] {
            let _ = result.remove(keyword);
        }
        normalize_dialect(&mut result, dialect, self.openapi30)?;
        self.legacy_format(&mut result, dialect, &mut targets);
        self.children(&mut result, &scoped, dialect, depth)?;
        if result.is_empty() && targets.len() == 1 {
            if let Some(target) = targets.pop() {
                let _ = result.insert("$ref".to_owned(), target);
            }
        } else if !targets.is_empty() {
            let branches = result.entry("allOf").or_insert_with(|| json!([]));
            let branches = branches
                .as_array_mut()
                .ok_or_else(|| Error::Schema("allOf must be an array.".to_owned()))?;
            branches.extend(targets.into_iter().map(|target| json!({"$ref":target})));
        }
        Ok(Value::Object(result))
    }

    fn legacy_format(
        &mut self,
        result: &mut Map<String, Value>,
        dialect: Dialect,
        targets: &mut Vec<Value>,
    ) {
        if dialect != Dialect::Draft4 {
            return;
        }
        if result.get("format").and_then(Value::as_str) == Some("hostname") {
            let _ = result.remove("format");
            let _ = self.definitions.entry("draft4_hostname").or_insert_with(|| {
                json!({
                    "$schema":"http://json-schema.org/draft-04/schema#",
                    "id":"https://riff.invalid/formats/draft4-hostname",
                    "$id":"https://riff.invalid/formats/draft4-hostname",
                    "format":"hostname"
                })
            });
            targets.push(json!("https://riff.invalid/formats/draft4-hostname"));
        }
        if let Some(kind) = result.get("type") {
            let integer = kind == "integer"
                || kind.as_array().is_some_and(|kinds| kinds.iter().any(|kind| kind == "integer"));
            if integer {
                let name = format!("draft4_type_{}", self.definitions.len());
                let uri = format!("https://riff.invalid/formats/{name}");
                let _ = self.definitions.insert(name, json!({"$schema":"http://json-schema.org/draft-04/schema#","id":uri,"$id":uri,"type":kind}));
                targets.push(json!(uri));
            }
        }
    }

    fn children(
        &mut self,
        result: &mut Map<String, Value>,
        scoped: &Resolver<'_>,
        dialect: Dialect,
        depth: usize,
    ) -> Result<()> {
        for (keyword, value) in result {
            if keyword == "$ref" {
                continue;
            }
            if !KEYWORDS.contains(&keyword.as_str())
                && !MAPS.contains(&keyword.as_str())
                && !LISTS.contains(&keyword.as_str())
                && !CHILDREN.contains(&keyword.as_str())
                && !keyword.starts_with("x-")
            {
                return Err(Error::Schema(format!("Unsupported schema keyword: {keyword}")));
            }
            if MAPS.contains(&keyword.as_str()) || keyword == "dependencies" {
                let children = value
                    .as_object_mut()
                    .ok_or_else(|| Error::Schema(format!("{keyword} must be an object.")))?;
                for child in children.values_mut() {
                    if keyword == "dependencies" && child.is_array() {
                        continue;
                    }
                    *child =
                        self.schema(child, scoped, dialect, depth + 1, ScopePosition::Inline)?;
                }
            } else if LISTS.contains(&keyword.as_str()) || (keyword == "items" && value.is_array())
            {
                let children = value
                    .as_array_mut()
                    .ok_or_else(|| Error::Schema(format!("{keyword} must be an array.")))?;
                for child in children {
                    *child =
                        self.schema(child, scoped, dialect, depth + 1, ScopePosition::Inline)?;
                }
            } else if CHILDREN.contains(&keyword.as_str()) {
                *value = self.schema(value, scoped, dialect, depth + 1, ScopePosition::Inline)?;
            }
        }
        Ok(())
    }

    fn reference(
        &mut self,
        reference: &str,
        keyword: &str,
        resolver: &Resolver<'_>,
        depth: usize,
    ) -> Result<Value> {
        // The private resource index must never create an addressable input pointer.
        if self.document_dialect.is_some() {
            let absolute = resolver
                .resolve_uri(&resolver.base_uri().borrow(), reference)
                .map_err(|error| Error::Schema(error.to_string()))?;
            if let Some(fragment) = absolute
                .as_str()
                .strip_prefix(DOCUMENT_URI)
                .and_then(|suffix| suffix.strip_prefix('#'))
                && fragment.starts_with('/')
            {
                let _ = crate::schema::resolve_reference(self.original, &format!("#{fragment}"))?;
            }
        }
        let mut scope = Vec::new();
        for name in &self.anchors {
            for uri in resolver
                .dynamic_scope()
                .into_iter()
                .map(|uri| uri.as_str().to_owned())
                .chain(std::iter::once(resolver.base_uri().as_str().to_owned()))
            {
                if let Ok(bound) = resolver.lookup(&format!("{uri}#{name}")) {
                    scope.push((name.clone(), std::ptr::from_ref(bound.contents())));
                    break;
                }
            }
        }
        let recursive_root =
            resolver.lookup_recursive_ref().map_err(|error| Error::Schema(error.to_string()))?;
        if recursive_root.contents().get("$recursiveAnchor") == Some(&Value::Bool(true)) {
            scope.push(("$recursiveRef".to_owned(), std::ptr::from_ref(recursive_root.contents())));
        }
        let key = Reference {
            base: resolver.base_uri().as_str().to_owned(),
            link: format!("{keyword}:{reference}"),
            scope,
        };
        if let Some(name) = self.references.get(&key) {
            return Ok(json!(format!("#/$defs/{name}")));
        }
        let registry = self.registry;
        let target = match keyword {
            "$recursiveRef" => resolver.lookup_recursive_ref(),
            "$dynamicRef" => resolver.lookup(reference),
            _ => {
                let absolute = resolver
                    .resolve_uri(&resolver.base_uri().borrow(), reference)
                    .map_err(|error| Error::Schema(error.to_string()))?;
                let (base, fragment) =
                    absolute.as_str().split_once('#').unwrap_or((absolute.as_str(), ""));
                let uri = jsonschema::uri::from_str(base)
                    .map_err(|error| Error::Schema(error.to_string()))?;
                registry.resolver(uri).lookup(&format!("#{fragment}"))
            }
        }
        .map_err(|error| Error::Schema(error.to_string()))?;
        let name = format!("reference_{}", self.references.len());
        let _ = self.references.insert(key, name.clone());
        let (contents, target_scope, draft) = target.into_inner();
        let nested = if keyword == "$ref" {
            resolver
                .lookup(target_scope.base_uri().as_str())
                .map_err(|error| Error::Schema(error.to_string()))?
                .into_inner()
                .1
        } else {
            target_scope
        };
        let dialect = if contents.get("$schema").is_none()
            && self.document_dialect.is_some()
            && nested.base_uri().as_str() == DOCUMENT_URI
        {
            self.document_dialect.unwrap_or_default()
        } else {
            match draft {
                Draft::Draft4 => Dialect::Draft4,
                Draft::Draft7 => Dialect::Draft7,
                Draft::Draft201909 => Dialect::Draft2019,
                _ => Dialect::Draft2020,
            }
        };
        let bundled = self.schema(contents, &nested, dialect, depth + 1, ScopePosition::Target)?;
        let _ = self.definitions.insert(name.clone(), bundled);
        Ok(json!(format!("#/$defs/{name}")))
    }
}

fn normalize_dialect(
    object: &mut Map<String, Value>,
    dialect: Dialect,
    openapi30: bool,
) -> Result<()> {
    if openapi30 || dialect == Dialect::Draft4 {
        crate::schema::normalize_openapi30(object)?;
    } else if object.contains_key("nullable") {
        return Err(Error::Schema("nullable requires OpenAPI 3.0; use a null type.".to_owned()));
    }
    let inactive: &[&str] = match dialect {
        Dialect::Draft4 | Dialect::Draft7 => &[
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
    if matches!(dialect, Dialect::Draft4 | Dialect::Draft7 | Dialect::Draft2019) {
        if let Some(Value::Array(items)) = object.get("items") {
            let items = items.clone();
            let additional = object.remove("additionalItems");
            let _ = object.insert("prefixItems".to_owned(), Value::Array(items));
            let _ = object.remove("items");
            if let Some(additional) = additional {
                let _ = object.insert("items".to_owned(), additional);
            }
        } else {
            let _ = object.remove("additionalItems");
        }
        if let Some(Value::Object(dependencies)) = object.remove("dependencies") {
            let mut required = Map::new();
            let mut schemas = Map::new();
            for (name, value) in dependencies {
                let _ = if value.is_array() {
                    required.insert(name, value)
                } else {
                    schemas.insert(name, value)
                };
            }
            let _ = object.insert("dependentRequired".to_owned(), Value::Object(required));
            let _ = object.insert("dependentSchemas".to_owned(), Value::Object(schemas));
        }
    }
    let inactive_formats: &[&str] = match dialect {
        Dialect::Draft4 => &[
            "uuid",
            "duration",
            "idn-hostname",
            "iri",
            "iri-reference",
            "json-pointer",
            "relative-json-pointer",
            "uri-reference",
            "uri-template",
        ],
        Dialect::Draft7 => &["uuid", "duration"],
        _ => &[],
    };
    if object
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| inactive_formats.contains(&format))
    {
        let _ = object.remove("format");
    }
    if dialect == Dialect::Draft4 {
        for keyword in ["const", "contains", "propertyNames", "if", "then", "else"] {
            let _ = object.remove(keyword);
        }
    }
    Ok(())
}

/// A generation hint and the complete-schema origin of each hinted node.
pub struct SamplingPlan {
    pub schema: Value,
    pub sources: BTreeMap<String, SchemaLocation>,
}

pub fn sampling_schema(
    schema: &Value,
    validators: &crate::node::Validators,
) -> Result<SamplingPlan> {
    let mut expansion = Expansion {
        document: schema,
        validators: Some(validators),
        terminals: BTreeMap::new(),
        sources: BTreeMap::new(),
    };
    let mut remaining = MAX_SCHEMA_NODES;
    let plan = expansion.expand(
        schema,
        &SchemaLocation::root(),
        "#",
        &BTreeSet::new(),
        &mut remaining,
        0,
    )?;
    Ok(SamplingPlan { schema: plan, sources: expansion.sources })
}

pub fn annotation_schema(schema: &Value) -> Result<Value> {
    let mut expansion = Expansion {
        document: schema,
        validators: None,
        terminals: BTreeMap::new(),
        sources: BTreeMap::new(),
    };
    let mut remaining = MAX_SCHEMA_NODES;
    let mut expanded = expansion.expand(
        schema,
        &SchemaLocation::root(),
        "#",
        &BTreeSet::new(),
        &mut remaining,
        0,
    )?;
    if let (Some(definitions), Some(object)) = (schema.get("$defs"), expanded.as_object_mut()) {
        let _ = object.insert("$defs".to_owned(), definitions.clone());
    }
    Ok(expanded)
}

struct Expansion<'a> {
    document: &'a Value,
    validators: Option<&'a crate::node::Validators>,
    terminals: BTreeMap<SchemaLocation, Value>,
    sources: BTreeMap<String, SchemaLocation>,
}

impl Expansion<'_> {
    fn expand(
        &mut self,
        input: &Value,
        source: &SchemaLocation,
        path: &str,
        references: &BTreeSet<String>,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<Value> {
        let _ = self.sources.insert(path.to_owned(), source.clone());
        if *remaining == 0 || depth > 32 {
            return if self.validators.is_some() {
                self.terminal(source)
            } else {
                Ok(input.clone())
            };
        }
        *remaining -= 1;
        let Some(object) = input.as_object() else { return Ok(input.clone()) };
        if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
            if references.contains(reference) || *remaining < 32 {
                return if self.validators.is_some() {
                    self.terminal(source)
                } else {
                    Ok(input.clone())
                };
            }
            let mut nested = references.clone();
            let _ = nested.insert(reference.to_owned());
            return self.expand(
                crate::schema::resolve_reference(self.document, reference)?,
                &SchemaLocation::from_reference(reference)?,
                path,
                &nested,
                remaining,
                depth + 1,
            );
        }
        let mut result = Value::Object(object.clone());
        if let Some(object) = result.as_object_mut() {
            let _ = object.remove("$defs");
        }
        let locations = crate::schema::child_locations(&result);
        let allowance = *remaining / locations.len().max(1);
        for location in locations {
            if let Some(child) = result.pointer_mut(&location) {
                let mut child_remaining = allowance;
                *child = self.expand(
                    child,
                    &source.child(&location),
                    &format!("{path}{location}"),
                    references,
                    &mut child_remaining,
                    depth + 1,
                )?;
                *remaining = remaining.saturating_sub(allowance - child_remaining);
            }
        }
        Ok(result)
    }

    fn terminal(&mut self, source: &SchemaLocation) -> Result<Value> {
        if let Some(schema) = self.terminals.get(source) {
            return Ok(schema.clone());
        }
        let validators = self
            .validators
            .ok_or_else(|| Error::Schema("Sampling requires schema validators.".to_owned()))?;
        let checker = validators.get(source)?;
        let mut candidates = vec![
            json!(false),
            json!(true),
            Value::Null,
            json!(0),
            json!(1),
            json!(""),
            json!([]),
            json!({}),
        ];
        let mut literal = source.resolve(self.document)?;
        let mut seen = BTreeSet::new();
        while let Some(reference) = literal.get("$ref").and_then(Value::as_str) {
            if !seen.insert(reference) {
                break;
            }
            literal = crate::schema::resolve_reference(self.document, reference)?;
        }
        if let Some(value) = literal.get("const") {
            candidates.push(value.clone());
        }
        if let Some(values) = literal.get("enum").and_then(Value::as_array) {
            candidates.extend(values.iter().take(64).cloned());
        }
        let values: Vec<_> =
            candidates.into_iter().filter(|value| checker.is_valid(value)).collect();
        let schema = if values.is_empty() { Value::Bool(false) } else { json!({"enum":values}) };
        let _ = self.terminals.insert(source.clone(), schema.clone());
        Ok(schema)
    }
}

fn dynamic_anchors(input: &Value) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending = vec![input];
    while let Some(value) = pending.pop() {
        if let Some(name) = value.get("$dynamicAnchor").and_then(Value::as_str) {
            let _ = names.insert(name.to_owned());
        }
        for location in crate::schema::child_locations(value) {
            if let Some(child) = value.pointer(&location) {
                pending.push(child);
            }
        }
    }
    names
}
