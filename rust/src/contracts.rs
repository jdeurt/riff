//! Complete declared contracts for provider callbacks, independent of sampling preferences.
use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::{Error, Result};

#[derive(Debug)]
pub enum DeclaredContract {
    Schema { value: Value, resources: Arc<Map<String, Value>> },
    Intersection(Arc<Self>, Arc<Self>),
}

impl DeclaredContract {
    pub fn render(&self) -> Result<Value> {
        let mut pending = vec![self];
        let mut schemas = Vec::new();
        let mut resources = Vec::new();
        while let Some(contract) = pending.pop() {
            match contract {
                Self::Schema { value, resources: definitions } => {
                    let mut schema = value.clone();
                    if let Some(object) = schema.as_object_mut() {
                        let _ = object.remove("$defs");
                    }
                    schemas.push(schema);
                    if !resources.iter().any(|existing| Arc::ptr_eq(existing, definitions)) {
                        resources.push(definitions.clone());
                    }
                }
                Self::Intersection(left, right) => {
                    pending.push(right);
                    pending.push(left);
                }
            }
        }
        let mut definitions = Map::new();
        for resource in resources {
            for (name, schema) in resource.iter() {
                if definitions.get(name).is_some_and(|existing| existing != schema) {
                    return Err(Error::Schema(
                        "Conflicting resources in provider contract.".to_owned(),
                    ));
                }
                let _ = definitions.insert(name.clone(), schema.clone());
            }
        }
        let mut schema =
            if schemas.len() == 1 { schemas.remove(0) } else { json!({"allOf":schemas}) };
        if !definitions.is_empty() {
            if !schema.is_object() {
                schema = json!({"allOf":[schema]});
            }
            if let Some(object) = schema.as_object_mut() {
                let _ = object.insert("$defs".to_owned(), Value::Object(definitions));
            }
        }
        Ok(schema)
    }
}
