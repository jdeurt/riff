//! Host controls translated without coercing JSON payloads or losing bigint seeds.
use neon::prelude::*;
use neon::types::{JsBigInt, Value as _};
use serde_json::{Map, Value};

use crate::{json, object_method};

pub fn object<'a>(cx: &mut FunctionContext<'a>, index: usize) -> JsResult<'a, JsObject> {
    let object = match cx.argument_opt(index) {
        Some(value) if !value.is_a::<JsUndefined, _>(cx) => {
            value.downcast_or_throw::<JsObject, _>(cx)?
        }
        _ => cx.empty_object(),
    };
    let prototype = object_method(cx, "getPrototypeOf", &[object.upcast()])?;
    let constructor = cx.global::<JsFunction>("Object")?;
    let expected = constructor.get::<JsObject, _, _>(cx, "prototype")?;
    let plain_prototype = prototype.strict_equals(cx, expected) || prototype.is_a::<JsNull, _>(cx);
    if object.is_a::<JsArray, _>(cx) || !plain_prototype {
        return cx.throw_range_error("Invocation options must be plain objects.");
    }
    Ok(object)
}

pub fn read<'a>(
    cx: &mut FunctionContext<'a>,
    object: Handle<'a, JsObject>,
    excluded: Option<&str>,
) -> NeonResult<Value> {
    let entries =
        object_method(cx, "entries", &[object.upcast()])?.downcast_or_throw::<JsArray, _>(cx)?;
    let mut controls = Map::new();
    for index in 0..entries.len(cx) {
        let entry = entries.get::<JsArray, _, _>(cx, index)?;
        let key = entry.get::<JsString, _, _>(cx, 0)?;
        let name = json::text(cx, key)?;
        let input = entry.get::<JsValue, _, _>(cx, 1)?;
        if excluded == Some(name.as_str()) || input.is_a::<JsUndefined, _>(cx) {
            continue;
        }
        let value = if let Ok(integer) = input.downcast::<JsBigInt, _>(cx) {
            let words = u64::try_from(integer.len(cx));
            if !words.is_ok_and(|words| words <= riff_core::MAX_INTEGER_BITS.div_ceil(64)) {
                return cx
                    .throw_range_error("Integer exceeds supported numeric allocation limits.");
            }
            Value::String(integer.to_string(cx)?.value(cx))
        } else {
            if let Ok(number) = input.downcast::<JsNumber, _>(cx)
                && !number.value(cx).is_finite()
            {
                return cx.throw_range_error("Invocation numbers must be finite.");
            }
            json::read(cx, input, &mut Vec::new())?
        };
        let _ = controls.insert(name, value);
    }
    Ok(Value::Object(controls))
}
