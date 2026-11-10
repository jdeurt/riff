//! Offline schema normalization, operation selection, and sidecar attachment.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Error, Metadata, Result};

pub const MAPS: &[&str] =
    &["properties", "patternProperties", "dependentSchemas", "$defs", "definitions"];
pub const LISTS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];
pub const CHILDREN: &[&str] = &[
    "items",
    "additionalItems",
    "additionalProperties",
    "not",
    "contains",
    "if",
    "then",
    "else",
    "propertyNames",
    "unevaluatedProperties",
    "unevaluatedItems",
    "contentSchema",
];
pub const KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "$ref",
    "$comment",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "xml",
    "externalDocs",
    "discriminator",
    "type",
    "enum",
    "const",
    "format",
    "pattern",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "required",
    "minProperties",
    "maxProperties",
    "minItems",
    "maxItems",
    "uniqueItems",
    "nullable",
    "minContains",
    "maxContains",
    "dependentRequired",
    "dependencies",
    "contentEncoding",
    "contentMediaType",
];

/// JSON Schema validation vocabulary selected for a contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dialect {
    /// Draft 4.
    Draft4,
    /// Draft 7.
    Draft7,
    /// Draft 2019-09.
    Draft2019,
    /// Draft 2020-12, including the `OpenAPI` 3.1 base dialect.
    #[default]
    Draft2020,
}

impl Dialect {
    pub(crate) fn select(value: &str) -> Result<Self> {
        match value.trim_end_matches('#') {
            "http://json-schema.org/draft-04/schema"
            | "https://json-schema.org/draft-04/schema" => Ok(Self::Draft4),
            "http://json-schema.org/draft-07/schema"
            | "https://json-schema.org/draft-07/schema" => Ok(Self::Draft7),
            "https://json-schema.org/draft/2019-09/schema" => Ok(Self::Draft2019),
            "https://json-schema.org/draft/2020-12/schema"
            | "https://spec.openapis.org/oas/3.1/dialect/base" => Ok(Self::Draft2020),
            _ => Err(Error::Schema(format!("Unsupported JSON Schema dialect: {value}"))),
        }
    }
}

/// Request/response vocabulary shared by `OpenAPI` and every sidecar selector.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Body {
    /// Select the operation's request body.
    Request,
    /// Select an exact response status.
    #[default]
    Response,
}

/// Exact `OpenAPI` body selection, shared with YAML operation overlays.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    /// Path template as written in the document.
    pub path: String,
    /// HTTP method, case-insensitive.
    #[serde(default = "default_method")]
    pub method: String,
    /// Request or response body.
    #[serde(default)]
    pub body: Body,
    /// Exact response status; ignored for request bodies.
    #[serde(default = "default_status")]
    pub status: String,
    /// Exact content-map media type.
    #[serde(default = "default_media_type")]
    pub media_type: String,
}

fn default_method() -> String {
    "get".to_owned()
}
fn default_status() -> String {
    "200".to_owned()
}
fn default_media_type() -> String {
    "application/json".to_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Overlay {
    version: String,
    #[serde(default)]
    fields: BTreeMap<String, Value>,
    #[serde(default)]
    operations: Vec<OverlayOperation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverlayOperation {
    path: String,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    body: Body,
    #[serde(default = "default_status")]
    status: String,
    #[serde(default = "default_media_type")]
    media_type: String,
    fields: BTreeMap<String, Value>,
}

/// Resolve a standalone or selected `OpenAPI` schema and attach a sidecar.
///
/// # Errors
/// Rejects unsupported dialects/keywords, missing resources, and ambiguous selectors.
pub fn prepare(
    document: &Value,
    operation: Option<&Operation>,
    overlay: Option<&Value>,
) -> Result<Value> {
    crate::limits::validate_numbers(document, 0)?;
    if let Some(overlay) = overlay {
        crate::limits::validate_numbers(overlay, 0)?;
    }
    let (selected, openapi30, inherited) = match operation {
        None => (document.clone(), false, None),
        Some(selector) => select_operation(document, selector)?,
    };
    let mut selected = selected;
    if let (Value::Object(object), Some(dialect)) = (&mut selected, &inherited) {
        let _ = object.entry("$schema").or_insert(Value::String(dialect.clone()));
    }
    let mut schema = crate::references::bundle(&selected, document, openapi30)?;
    if let Some(overlay) = overlay {
        schema = crate::references::annotation_schema(&schema)?;
        attach(&mut schema, overlay, operation)?;
    }
    Ok(schema)
}

fn select_operation(
    document: &Value,
    selector: &Operation,
) -> Result<(Value, bool, Option<String>)> {
    let version = document
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Schema("Expected an OpenAPI 3.0 or 3.1 document.".to_owned()))?;
    if !version.starts_with("3.0.") && !version.starts_with("3.1.") {
        return Err(Error::Schema("Expected an OpenAPI 3.0 or 3.1 document.".to_owned()));
    }
    let paths = object_reference(document, document.get("paths"))?;
    let path = object_reference(document, paths.get(&selector.path))?;
    let operation = object_reference(document, path.get(&selector.method.to_lowercase()))?;
    let selected = match selector.body {
        Body::Request => object_reference(document, operation.get("requestBody"))?,
        Body::Response => {
            let responses = object_reference(document, operation.get("responses"))?;
            object_reference(document, responses.get(&selector.status))?
        }
    };
    let content = object_reference(document, selected.get("content"))?;
    let media = object_reference(document, content.get(&selector.media_type))?;
    let schema = media
        .get("schema")
        .cloned()
        .ok_or_else(|| Error::Schema("Selected body has no schema.".to_owned()))?;
    let dialect = document
        .get("jsonSchemaDialect")
        .map(|value| {
            let uri = value
                .as_str()
                .ok_or_else(|| Error::Schema("jsonSchemaDialect must be a URI.".to_owned()))?;
            let _ = Dialect::select(uri)?;
            Ok::<_, Error>(uri.to_owned())
        })
        .transpose()?;
    Ok((schema, version.starts_with("3.0."), dialect))
}

fn object_reference(document: &Value, input: Option<&Value>) -> Result<Map<String, Value>> {
    let mut current = input
        .cloned()
        .ok_or_else(|| Error::Schema("Selected OpenAPI body was not found.".to_owned()))?;
    let mut seen = BTreeSet::new();
    loop {
        let Value::Object(object) = current else {
            return Err(Error::Schema("Expected an OpenAPI object.".to_owned()));
        };
        let Some(reference) = object.get("$ref") else { return Ok(object) };
        let reference = reference
            .as_str()
            .ok_or_else(|| Error::Schema("Invalid OpenAPI object reference.".to_owned()))?;
        if !seen.insert(reference.to_owned()) {
            return Err(Error::Schema("Recursive OpenAPI object reference.".to_owned()));
        }
        current = resolve_reference(document, reference)?.clone();
    }
}

pub fn resolve_reference<'a>(document: &'a Value, reference: &str) -> Result<&'a Value> {
    let Some(local) = reference.strip_prefix('#') else {
        let mut pending = vec![document];
        while let Some(schema) = pending.pop() {
            if schema
                .get("$id")
                .or_else(|| schema.get("id"))
                .and_then(Value::as_str)
                .is_some_and(|uri| uri.trim_end_matches('#') == reference.trim_end_matches('#'))
            {
                return Ok(schema);
            }
            for location in child_locations(schema) {
                if let Some(child) = schema.pointer(&location) {
                    pending.push(child);
                }
            }
        }
        return Err(Error::Schema(format!("Unresolved supplied resource: {reference}")));
    };
    let decoded = percent_encoding::percent_decode_str(local)
        .decode_utf8()
        .map_err(|error| Error::Schema(error.to_string()))?;
    validate_pointer(&decoded)?;
    document
        .pointer(&decoded)
        .ok_or_else(|| Error::Schema(format!("Unresolved reference: {reference}")))
}

fn validate_pointer(pointer: &str) -> Result<()> {
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return Err(Error::Schema("Invalid schema path: expected RFC 6901 pointer.".to_owned()));
    }
    let mut characters = pointer.chars();
    while let Some(character) = characters.next() {
        if character == '~' && !matches!(characters.next(), Some('0' | '1')) {
            return Err(Error::Schema("Invalid schema path: malformed pointer escape.".to_owned()));
        }
    }
    Ok(())
}

pub fn normalize_openapi30(object: &mut Map<String, Value>) -> Result<()> {
    for (exclusive, inclusive) in [("exclusiveMinimum", "minimum"), ("exclusiveMaximum", "maximum")]
    {
        let value = object.remove(exclusive).unwrap_or(Value::Bool(false));
        match value {
            Value::Bool(true) => {
                let bound = object
                    .remove(inclusive)
                    .ok_or_else(|| Error::Schema(format!("{exclusive} requires {inclusive}.")))?;
                let _ = object.insert(exclusive.to_owned(), bound);
            }
            Value::Bool(false) => {}
            _ => {
                return Err(Error::Schema(
                    "OpenAPI 3.0 exclusive bounds must be booleans.".to_owned(),
                ));
            }
        }
    }
    let nullable = object.remove("nullable").unwrap_or(Value::Bool(false));
    match nullable {
        Value::Bool(true) => {
            if let Some(Value::String(kind)) = object.get("type") {
                let value = Value::Array(vec![
                    Value::String(kind.clone()),
                    Value::String("null".to_owned()),
                ]);
                let _ = object.insert("type".to_owned(), value);
            }
        }
        Value::Bool(false) => {}
        _ => return Err(Error::Schema("nullable must be a boolean.".to_owned())),
    }
    Ok(())
}

pub fn child_locations(schema: &Value) -> Vec<String> {
    let Some(object) = schema.as_object() else { return Vec::new() };
    let mut locations = Vec::new();
    for (keyword, value) in object {
        let prefix = format!("/{}", pointer_token(keyword));
        if MAPS.contains(&keyword.as_str()) || keyword == "dependencies" {
            if let Some(children) = value.as_object() {
                for (name, child) in children {
                    if keyword == "dependencies" && child.is_array() {
                        continue;
                    }
                    locations.push(format!("{prefix}/{}", pointer_token(name)));
                }
            }
        } else if LISTS.contains(&keyword.as_str()) || (keyword == "items" && value.is_array()) {
            if let Some(children) = value.as_array() {
                locations.extend((0..children.len()).map(|index| format!("{prefix}/{index}")));
            }
        } else if CHILDREN.contains(&keyword.as_str()) {
            locations.push(prefix);
        }
    }
    locations
}

fn schema_paths(schema: &Value, prefix: &str, output: &mut BTreeSet<String>) {
    if !schema.is_object() {
        return;
    }
    let _ = output.insert(prefix.to_owned());
    for location in child_locations(schema) {
        if let Some(child) = schema.pointer(&location) {
            schema_paths(child, &format!("{prefix}{location}"), output);
        }
    }
}

fn attach(schema: &mut Value, input: &Value, selector: Option<&Operation>) -> Result<()> {
    let overlay: Overlay = crate::documents::decode(input)?;
    if overlay.version != "0.1" {
        return Err(Error::Schema("Unsupported fixture sidecar version.".to_owned()));
    }
    let mut fields = overlay.fields;
    for entry in fields
        .values()
        .chain(overlay.operations.iter().flat_map(|operation| operation.fields.values()))
    {
        let _ = Metadata::parse(Some(entry))?;
    }
    if !overlay.operations.is_empty() {
        let selector = selector.ok_or_else(|| {
            Error::Schema("Operation annotations require OpenAPI selection.".to_owned())
        })?;
        let matches: Vec<_> = overlay
            .operations
            .into_iter()
            .filter(|operation| {
                operation.path == selector.path
                    && operation.method.eq_ignore_ascii_case(&selector.method)
                    && operation.body == selector.body
                    && operation.media_type == selector.media_type
                    && (selector.body == Body::Request || operation.status == selector.status)
            })
            .collect();
        if matches.len() != 1 {
            return Err(Error::Schema(
                "Fixture sidecar must match exactly one operation.".to_owned(),
            ));
        }
        if let Some(operation) = matches.into_iter().next() {
            fields.extend(operation.fields);
        }
    }
    let mut locations = BTreeSet::new();
    schema_paths(schema, "", &mut locations);
    for (pointer, metadata) in fields {
        validate_pointer(&pointer)?;
        if !locations.contains(&pointer) {
            return Err(Error::Schema(format!("Invalid fixture schema path: {pointer}")));
        }
        let target =
            schema.pointer_mut(&pointer).and_then(Value::as_object_mut).ok_or_else(|| {
                Error::Schema("Fixture schema path requires an object schema.".to_owned())
            })?;
        let _ = target.insert("x-fixture".to_owned(), metadata);
    }
    Ok(())
}

pub fn pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Remove schema fixture annotations without changing examples, defaults, or literal property names.
#[must_use]
pub fn strip_metadata(document: &Value) -> Value {
    match document {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let cleaned = if key == "schema" {
                        strip_schema(value)
                    } else if key == "schemas" {
                        value.as_object().map_or_else(
                            || value.clone(),
                            |schemas| {
                                Value::Object(
                                    schemas
                                        .iter()
                                        .map(|(name, child)| (name.clone(), strip_schema(child)))
                                        .collect(),
                                )
                            },
                        )
                    } else if matches!(key.as_str(), "example" | "examples" | "default")
                        || key.starts_with("x-")
                    {
                        value.clone()
                    } else {
                        strip_metadata(value)
                    };
                    (key.clone(), cleaned)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(strip_metadata).collect()),
        _ => document.clone(),
    }
}

fn strip_schema(schema: &Value) -> Value {
    let mut result = schema.clone();
    for location in child_locations(schema) {
        if let Some(child) = result.pointer_mut(&location) {
            *child = strip_schema(child);
        }
    }
    if let Some(object) = result.as_object_mut() {
        let _ = object.remove("x-fixture");
    }
    result
}
