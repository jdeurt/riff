//! Discover Schema Objects without interpreting `OpenAPI` example or extension data.
use serde_json::{Map, Value, json};

use crate::{Dialect, Error, Result};

#[derive(Clone, Copy)]
enum ObjectKind {
    Document,
    Components,
    PathItem,
    Operation,
    RequestBody,
    Response,
    Parameter,
    MediaType,
    Encoding,
    Callback,
    Schema,
}

#[derive(Clone, Copy)]
enum Container {
    Object,
    Map,
    Array,
}

struct Edge {
    field: &'static str,
    kind: ObjectKind,
    container: Container,
}

use Container::{Array, Map as Named, Object};
use ObjectKind::{
    Callback, Components, Encoding, MediaType, Operation, Parameter, PathItem, RequestBody,
    Response, Schema,
};

const DOCUMENT: &[Edge] = &[
    Edge { field: "components", kind: Components, container: Object },
    Edge { field: "paths", kind: PathItem, container: Named },
    Edge { field: "webhooks", kind: PathItem, container: Named },
];

const COMPONENTS: &[Edge] = &[
    Edge { field: "schemas", kind: Schema, container: Named },
    Edge { field: "parameters", kind: Parameter, container: Named },
    Edge { field: "headers", kind: Parameter, container: Named },
    Edge { field: "requestBodies", kind: RequestBody, container: Named },
    Edge { field: "responses", kind: Response, container: Named },
    Edge { field: "callbacks", kind: Callback, container: Named },
    Edge { field: "pathItems", kind: PathItem, container: Named },
];

const PATH_ITEM: &[Edge] = &[
    Edge { field: "get", kind: Operation, container: Object },
    Edge { field: "post", kind: Operation, container: Object },
    Edge { field: "put", kind: Operation, container: Object },
    Edge { field: "patch", kind: Operation, container: Object },
    Edge { field: "delete", kind: Operation, container: Object },
    Edge { field: "head", kind: Operation, container: Object },
    Edge { field: "options", kind: Operation, container: Object },
    Edge { field: "trace", kind: Operation, container: Object },
    Edge { field: "parameters", kind: Parameter, container: Array },
];

const OPERATION: &[Edge] = &[
    Edge { field: "parameters", kind: Parameter, container: Array },
    Edge { field: "requestBody", kind: RequestBody, container: Object },
    Edge { field: "responses", kind: Response, container: Named },
    Edge { field: "callbacks", kind: Callback, container: Named },
];

const REQUEST_BODY: &[Edge] = &[Edge { field: "content", kind: MediaType, container: Named }];

const RESPONSE: &[Edge] = &[
    Edge { field: "content", kind: MediaType, container: Named },
    Edge { field: "headers", kind: Parameter, container: Named },
];

const PARAMETER: &[Edge] = &[
    Edge { field: "schema", kind: Schema, container: Object },
    Edge { field: "content", kind: MediaType, container: Named },
];

const MEDIA_TYPE: &[Edge] = &[
    Edge { field: "schema", kind: Schema, container: Object },
    Edge { field: "encoding", kind: Encoding, container: Named },
];

const ENCODING: &[Edge] = &[Edge { field: "headers", kind: Parameter, container: Named }];

const fn edges(kind: ObjectKind) -> &'static [Edge] {
    match kind {
        ObjectKind::Document => DOCUMENT,
        ObjectKind::Components => COMPONENTS,
        ObjectKind::PathItem => PATH_ITEM,
        ObjectKind::Operation => OPERATION,
        ObjectKind::RequestBody => REQUEST_BODY,
        ObjectKind::Response => RESPONSE,
        ObjectKind::Parameter => PARAMETER,
        ObjectKind::MediaType => MEDIA_TYPE,
        ObjectKind::Encoding => ENCODING,
        Callback | Schema => &[],
    }
}

pub struct SchemaIndex {
    pub document: Value,
    pub resources: Vec<(String, Value)>,
}

pub fn indexed_document(
    selected: &Value,
    document: &Value,
    dialect: Option<Dialect>,
    document_uri: &str,
) -> Result<SchemaIndex> {
    let Some(dialect) = dialect else {
        return Ok(SchemaIndex { document: document.clone(), resources: Vec::new() });
    };
    let uri = match dialect {
        Dialect::Draft4 => "http://json-schema.org/draft-04/schema#",
        Dialect::Draft7 => "http://json-schema.org/draft-07/schema#",
        Dialect::Draft2019 => "https://json-schema.org/draft/2019-09/schema",
        Dialect::Draft2020 => "https://json-schema.org/draft/2020-12/schema",
    };
    let mut schemas = vec![selected.clone()];
    let mut pending = vec![(ObjectKind::Document, document)];
    while let Some((kind, value)) = pending.pop() {
        if matches!(kind, ObjectKind::Schema) {
            schemas.push(value.clone());
            continue;
        }
        if matches!(kind, ObjectKind::Callback) {
            if let Some(entries) = value.as_object() {
                pending.extend(
                    entries
                        .iter()
                        .filter(|(name, _)| !name.starts_with("x-") && *name != "$ref")
                        .map(|(_, child)| (ObjectKind::PathItem, child)),
                );
            }
            continue;
        }
        for edge in edges(kind) {
            let Some(child) = value.get(edge.field) else { continue };
            match edge.container {
                Container::Object => pending.push((edge.kind, child)),
                Container::Map => {
                    if let Some(entries) = child.as_object() {
                        pending.extend(
                            entries
                                .iter()
                                .filter(|(name, _)| {
                                    !matches!(edge.field, "paths" | "webhooks" | "responses")
                                        || !name.starts_with("x-")
                                })
                                .map(|(_, child)| (edge.kind, child)),
                        );
                    }
                }
                Container::Array => {
                    if let Some(entries) = child.as_array() {
                        pending.extend(entries.iter().map(|child| (edge.kind, child)));
                    }
                }
            }
        }
    }
    for schema in &mut schemas {
        if let Some(object) = schema.as_object_mut() {
            let _ = object.entry("$schema").or_insert_with(|| json!(uri));
        }
    }
    let base = jsonschema::uri::from_str(document_uri)
        .map_err(|error| Error::Schema(error.to_string()))?;
    let mut resources = Vec::new();
    for schema in &schemas {
        let declared = schema
            .get("$schema")
            .and_then(Value::as_str)
            .map(Dialect::select)
            .transpose()?
            .unwrap_or(dialect);
        let name = if declared == Dialect::Draft4 { "id" } else { "$id" };
        if let Some(id) = schema.get(name).and_then(Value::as_str) {
            let uri = referencing::uri::resolve_against(&base.borrow(), id)
                .map_err(|error| Error::Schema(error.to_string()))?;
            resources.push((uri.as_str().to_owned(), schema.clone()));
        }
    }
    let mut indexed = document.clone();
    let object = indexed
        .as_object_mut()
        .ok_or_else(|| Error::Schema("Expected an OpenAPI document.".to_owned()))?;
    for container in ["$defs", "definitions"] {
        if object.get(container).is_none_or(Value::is_object) {
            let entries = object.entry(container).or_insert_with(|| Value::Object(Map::new()));
            if let Some(entries) = entries.as_object_mut() {
                let mut index = 0;
                for schema in schemas {
                    while entries.contains_key(&format!("schema_{index}")) {
                        index += 1;
                    }
                    let _ = entries.insert(format!("schema_{index}"), schema);
                }
            }
            return Ok(SchemaIndex { document: indexed, resources });
        }
    }
    Err(Error::Schema(
        "OpenAPI schema-resource indexing requires object-valued definitions.".to_owned(),
    ))
}
