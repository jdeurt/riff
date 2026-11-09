//! Reachable failure declarations and single-location mutation.
use std::collections::BTreeSet;
use std::str::FromStr;

use bigdecimal::BigDecimal;

use serde::Serialize;
use serde_json::Value;

use crate::engine::{Engine, ProviderContext};
use crate::node::{Node, SchemaNode};
use crate::scalars::{choose, within_limits};
use crate::schema::pointer_token;
use crate::{Error, Result, Source};

/// One independent validator diagnostic.
#[derive(Clone, Debug, Serialize)]
pub struct Violation {
    /// RFC 6901 payload pointer.
    pub path: String,
    /// Backend-specific explanation.
    pub message: String,
}

/// A labeled raw fixture; declared outcome is independent of schema validity.
#[derive(Clone, Debug, Serialize)]
pub struct Case {
    /// Exact data to supply to the system under test.
    pub value: Value,
    /// Caller-declared pass or fail expectation.
    pub expectation: String,
    /// Observed validity against the original contract.
    pub schema_valid: bool,
    /// Independent validator diagnostics.
    pub violations: Vec<Violation>,
    /// Mutation location, absent for successful data.
    pub target: Option<String>,
    /// Consumer-owned explanation, if supplied.
    pub reason: Option<String>,
}

#[derive(Clone)]
struct Target<'a> {
    pointer: String,
    node: &'a SchemaNode,
    source: &'a Source,
}

pub fn failure<F: FnMut(&ProviderContext) -> Result<Value>>(
    node: &Node,
    engine: &mut Engine<'_, F>,
) -> Result<Case> {
    for _ in 0..engine.max_attempts {
        let base = engine.generate(node)?;
        let mut declarations = Vec::new();
        targets(node, &base, "", &mut declarations)?;
        if declarations.is_empty() {
            continue;
        }
        let target = choose(&declarations, &mut engine.random)?.clone();
        engine.reset();
        let replacement = engine.source(target.source, target.node, 0)?;
        if !within_limits(&replacement, 0) {
            continue;
        }
        let mut mutated = base.clone();
        replace(&mut mutated, &target.pointer, replacement)?;
        if !within_limits(&mutated, 0) || equivalent(&base, &mutated) {
            continue;
        }
        let violations = match node {
            Node::Boolean(_) => Vec::new(),
            Node::Schema(node) => node
                .validator
                .iter_errors(&mutated)
                .map(|error| Violation {
                    path: error.instance_path().to_string(),
                    message: error.to_string(),
                })
                .collect(),
        };
        return Ok(Case {
            value: mutated,
            expectation: "fail".to_owned(),
            schema_valid: violations.is_empty(),
            violations,
            target: Some(target.pointer),
            reason: target.source.reason.clone(),
        });
    }
    Err(Error::Generation(
        "No reachable failure declaration produced a changed fixture within the attempt budget."
            .to_owned(),
    ))
}

fn targets<'a>(
    node: &'a Node,
    value: &Value,
    pointer: &str,
    output: &mut Vec<Target<'a>>,
) -> Result<()> {
    let Node::Schema(node) = node else { return Ok(()) };
    if let Some(source) = &node.metadata.failure {
        output.push(Target { pointer: pointer.to_owned(), node, source });
    }
    if let Value::Object(object) = value {
        for (name, child) in &node.properties {
            targets(
                child,
                object.get(name).unwrap_or(&Value::Null),
                &format!("{pointer}/{}", pointer_token(name)),
                output,
            )?;
        }
        for (name, value) in object {
            let location = format!("{pointer}/{}", pointer_token(name));
            let matches = node.matching_patterns(name)?;
            if !node.properties.contains_key(name) && matches.is_empty() {
                targets(&node.additional, value, &location, output)?;
            }
            for child in matches {
                targets(child, value, &location, output)?;
            }
        }
        if let Some(unevaluated) = &node.unevaluated_properties {
            let evaluated = evaluated_keys(node, object, false)?;
            for (name, child) in object {
                if !evaluated.contains(name) {
                    targets(
                        unevaluated,
                        child,
                        &format!("{pointer}/{}", pointer_token(name)),
                        output,
                    )?;
                }
            }
        }
    }
    if let Value::Array(values) = value {
        let evaluated = evaluated_items(node, values, false)?;
        for (index, child) in values.iter().enumerate() {
            let location = format!("{pointer}/{index}");
            let declared = node.tuple.get(index).unwrap_or_else(|| {
                if node.contract.get("items").is_some_and(Value::is_array) {
                    &node.additional_items
                } else {
                    &node.items
                }
            });
            targets(declared, child, &location, output)?;
            if let Some(contains) = &node.contains
                && contains.valid(child)
            {
                targets(contains, child, &location, output)?;
            }
            if let Some(unevaluated) = &node.unevaluated_items
                && !evaluated.contains(&index)
            {
                targets(unevaluated, child, &location, output)?;
            }
        }
    }
    for branch in applicable(node, value) {
        targets(branch, value, pointer, output)?;
    }
    Ok(())
}

fn applicable<'a>(node: &'a SchemaNode, value: &Value) -> Vec<&'a Node> {
    let mut branches: Vec<_> = node
        .all_of
        .iter()
        .chain(node.any_of.iter().chain(&node.one_of).filter(|child| child.valid(value)))
        .collect();
    if let Some(condition) = &node.condition {
        if condition.valid(value) {
            branches.push(condition);
            branches.extend(node.then_schema.iter());
        } else {
            branches.extend(node.else_schema.iter());
        }
    }
    if let Value::Object(object) = value {
        branches.extend(
            node.dependencies
                .iter()
                .filter(|(name, _)| object.contains_key(*name))
                .map(|(_, child)| child),
        );
    }
    branches
}

fn evaluated_keys(
    node: &SchemaNode,
    object: &serde_json::Map<String, Value>,
    include_unevaluated: bool,
) -> Result<BTreeSet<String>> {
    if node.applied.additional || (include_unevaluated && node.unevaluated_properties.is_some()) {
        return Ok(object.keys().cloned().collect());
    }
    let mut evaluated = BTreeSet::new();
    for name in object.keys() {
        if node.properties.contains_key(name) || !node.matching_patterns(name)?.is_empty() {
            let _ = evaluated.insert(name.clone());
        }
    }
    let value = Value::Object(object.clone());
    for branch in applicable(node, &value) {
        if let Node::Schema(branch) = branch {
            evaluated.extend(evaluated_keys(branch, object, true)?);
        }
    }
    Ok(evaluated)
}

fn evaluated_items(
    node: &SchemaNode,
    values: &[Value],
    include_unevaluated: bool,
) -> Result<BTreeSet<usize>> {
    if node.applied.items
        || (node.contract.get("items").is_some_and(Value::is_array)
            && node.applied.additional_items)
        || (include_unevaluated && node.unevaluated_items.is_some())
    {
        return Ok((0..values.len()).collect());
    }
    let mut evaluated: BTreeSet<_> = (0..node.tuple.len().min(values.len())).collect();
    if let Some(contains) = &node.contains {
        evaluated.extend(
            values
                .iter()
                .enumerate()
                .filter(|(_, value)| contains.valid(value))
                .map(|(index, _)| index),
        );
    }
    let value = Value::Array(values.to_vec());
    for branch in applicable(node, &value) {
        if let Node::Schema(branch) = branch {
            evaluated.extend(evaluated_items(branch, values, true)?);
        }
    }
    Ok(evaluated)
}

pub fn replace(value: &mut Value, pointer: &str, replacement: Value) -> Result<()> {
    if pointer.is_empty() {
        *value = replacement;
        return Ok(());
    }
    let (parent, token) = pointer
        .rsplit_once('/')
        .ok_or_else(|| Error::Generation("Unreachable failure target.".to_owned()))?;
    let name = token.replace("~1", "/").replace("~0", "~");
    let parent = value
        .pointer_mut(parent)
        .ok_or_else(|| Error::Generation("Unreachable failure parent.".to_owned()))?;
    match parent {
        Value::Object(object) => {
            let _ = object.insert(name, replacement);
        }
        Value::Array(array) => {
            let index =
                name.parse::<usize>().map_err(|error| Error::Generation(error.to_string()))?;
            let child = array
                .get_mut(index)
                .ok_or_else(|| Error::Generation("Unreachable array target.".to_owned()))?;
            *child = replacement;
        }
        _ => return Err(Error::Generation("Unreachable failure target.".to_owned())),
    }
    Ok(())
}

pub fn equivalent(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            match (
                BigDecimal::from_str(&left.to_string()),
                BigDecimal::from_str(&right.to_string()),
            ) {
                (Ok(left), Ok(right)) => left == right,
                _ => left == right,
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| equivalent(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(name, value)| {
                    right.get(name).is_some_and(|other| equivalent(value, other))
                })
        }
        _ => left == right,
    }
}
