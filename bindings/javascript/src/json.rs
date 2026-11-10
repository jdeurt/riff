//! Convert JavaScript data without invoking payload accessors or serialization hooks.
use neon::prelude::*;
use serde_json::{Map, Number, Value};
use std::str::FromStr;

use super::{json_value, object_method};
use crate::errors::raise as throw_core;

pub fn copy(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsValue>(0)?;
    let value = read(&mut cx, input, &mut Vec::new())?;
    let source = match serde_json::to_string(&value) {
        Ok(source) => source,
        Err(error) => return throw_core(&mut cx, error.into()),
    };
    json_value(&mut cx, &source)
}

pub fn read<'a>(
    cx: &mut FunctionContext<'a>,
    input: Handle<'a, JsValue>,
    ancestors: &mut Vec<Handle<'a, JsObject>>,
) -> NeonResult<Value> {
    if ancestors.len() > riff_core::MAX_INPUT_DEPTH {
        return throw_core(
            cx,
            riff_core::Error::Schema("JSON nesting exceeds 128 levels.".to_owned()),
        );
    }
    if input.is_a::<JsNull, _>(cx) {
        return Ok(Value::Null);
    }
    if let Ok(value) = input.downcast::<JsBoolean, _>(cx) {
        return Ok(Value::Bool(value.value(cx)));
    }
    if let Ok(value) = input.downcast::<JsString, _>(cx) {
        return text(cx, value).map(Value::String);
    }
    if let Ok(value) = input.downcast::<JsNumber, _>(cx) {
        let number = value.value(cx);
        if number.is_finite() {
            return Number::from_str(&number.to_string())
                .map(Value::Number)
                .or_else(|error| throw_core(cx, error.into()));
        }
    }
    let Ok(object) = input.downcast::<JsObject, _>(cx) else {
        return throw_core(cx, riff_core::Error::Schema("Expected finite JSON data.".to_owned()));
    };
    if ancestors.iter().any(|ancestor| ancestor.strict_equals(cx, object)) {
        return throw_core(cx, riff_core::Error::Schema("JSON data must be acyclic.".to_owned()));
    }
    let array = input.downcast::<JsArray, _>(cx).ok();
    validate_prototype(cx, object, array.is_some())?;
    ancestors.push(object);
    let value = container(cx, object, array, ancestors)?;
    let _ = ancestors.pop();
    Ok(value)
}

pub fn text<'a>(cx: &mut FunctionContext<'a>, input: Handle<'a, JsString>) -> NeonResult<String> {
    let decoded = input.value(cx);
    let reconstructed = cx.string(&decoded);
    if !reconstructed.strict_equals(cx, input) {
        return throw_core(
            cx,
            riff_core::Error::Schema(
                "JSON strings must contain valid Unicode scalar values.".to_owned(),
            ),
        );
    }
    Ok(decoded)
}

fn validate_prototype<'a>(
    cx: &mut FunctionContext<'a>,
    object: Handle<'a, JsObject>,
    array: bool,
) -> NeonResult<()> {
    let prototype = object_method(cx, "getPrototypeOf", &[object.upcast()])?;
    let constructor = cx.global::<JsFunction>(if array { "Array" } else { "Object" })?;
    let expected = constructor.get::<JsValue, _, _>(cx, "prototype")?;
    if prototype.strict_equals(cx, expected) || (!array && prototype.is_a::<JsNull, _>(cx)) {
        return Ok(());
    }
    throw_core(
        cx,
        riff_core::Error::Schema("JSON objects and arrays must have plain prototypes.".to_owned()),
    )
}

fn container<'a>(
    cx: &mut FunctionContext<'a>,
    object: Handle<'a, JsObject>,
    array: Option<Handle<'a, JsArray>>,
    ancestors: &mut Vec<Handle<'a, JsObject>>,
) -> NeonResult<Value> {
    let reflect = cx.global::<JsObject>("Reflect")?;
    let own_keys = reflect.get::<JsFunction, _, _>(cx, "ownKeys")?;
    let keys =
        own_keys.call(cx, reflect, [object.upcast()])?.downcast_or_throw::<JsArray, _>(cx)?;
    if let Some(array) = array {
        let length = array.len(cx);
        if keys.len(cx).checked_sub(1) != Some(length) {
            return throw_core(
                cx,
                riff_core::Error::Schema(
                    "JSON arrays must be dense and contain only indexed data.".to_owned(),
                ),
            );
        }
        let mut values = Vec::new();
        for index in 0..length {
            let key = cx.string(index.to_string());
            let value = property(cx, object, key.upcast(), ancestors)?;
            values.push(value);
        }
        return Ok(Value::Array(values));
    }
    let mut values = Map::new();
    for index in 0..keys.len(cx) {
        let key = keys.get::<JsValue, _, _>(cx, index)?;
        let Ok(name) = key.downcast::<JsString, _>(cx) else {
            return throw_core(
                cx,
                riff_core::Error::Schema("JSON object keys must be strings.".to_owned()),
            );
        };
        let value = property(cx, object, key, ancestors)?;
        let name = text(cx, name)?;
        let _ = values.insert(name, value);
    }
    Ok(Value::Object(values))
}

fn property<'a>(
    cx: &mut FunctionContext<'a>,
    object: Handle<'a, JsObject>,
    key: Handle<'a, JsValue>,
    ancestors: &mut Vec<Handle<'a, JsObject>>,
) -> NeonResult<Value> {
    let descriptor = object_method(cx, "getOwnPropertyDescriptor", &[object.upcast(), key])?;
    let Ok(descriptor) = descriptor.downcast::<JsObject, _>(cx) else {
        return throw_core(
            cx,
            riff_core::Error::Schema("JSON arrays cannot contain holes.".to_owned()),
        );
    };
    let enumerable = descriptor.get::<JsBoolean, _, _>(cx, "enumerable")?.value(cx);
    let getter = descriptor.get::<JsValue, _, _>(cx, "get")?;
    let setter = descriptor.get::<JsValue, _, _>(cx, "set")?;
    if !enumerable || !getter.is_a::<JsUndefined, _>(cx) || !setter.is_a::<JsUndefined, _>(cx) {
        return throw_core(
            cx,
            riff_core::Error::Schema(
                "JSON properties must contain enumerable data, not accessors.".to_owned(),
            ),
        );
    }
    let value = descriptor.get::<JsValue, _, _>(cx, "value")?;
    read(cx, value, ancestors)
}
