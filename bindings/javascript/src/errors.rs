//! Native error constructors with JavaScript inheritance and caller-owned causes.
use neon::prelude::*;

use crate::{api, object_method};

pub fn construct<'a, C: Context<'a>>(
    cx: &mut C,
    name: &str,
    message: Handle<'a, JsValue>,
    options: Handle<'a, JsValue>,
) -> JsResult<'a, JsObject> {
    let builtin = cx.global::<JsFunction>("Error")?;
    let error = builtin.construct(cx, [message, options])?;
    let constructor = api(cx)?.get::<JsFunction, _, _>(cx, name)?;
    let prototype = constructor.get::<JsObject, _, _>(cx, "prototype")?;
    let _ = object_method(cx, "setPrototypeOf", &[error.upcast(), prototype.upcast()])?;
    let name = cx.string(name);
    let _ = error.set(cx, "name", name)?;
    Ok(error)
}

fn constructor<'a>(mut cx: FunctionContext<'a>, name: &str) -> JsResult<'a, JsObject> {
    if matches!(cx.kind(), neon::context::CallKind::Call) {
        return cx.throw_type_error(format!("Class constructor {name} requires new."));
    }
    let message = cx.argument_opt(0).unwrap_or_else(|| cx.undefined().upcast());
    let options = cx.argument_opt(1).unwrap_or_else(|| cx.undefined().upcast());
    let error = construct(&mut cx, name, message, options)?;
    let receiver = cx.this::<JsObject>()?;
    let prototype = object_method(&mut cx, "getPrototypeOf", &[receiver.upcast()])?;
    let _ = object_method(&mut cx, "setPrototypeOf", &[error.upcast(), prototype])?;
    Ok(error)
}

pub fn raise<T>(cx: &mut FunctionContext<'_>, error: riff_core::Error) -> NeonResult<T> {
    let (name, message) = match error {
        riff_core::Error::Argument(message) => return cx.throw_range_error(message),
        riff_core::Error::Schema(message) => ("SchemaError", message),
        riff_core::Error::Generation(message) => ("GenerationError", message),
        riff_core::Error::Provider(message) => ("ProviderError", message),
    };
    let message = cx.string(message).upcast();
    let options = cx.undefined().upcast();
    let error = construct(cx, name, message, options)?;
    cx.throw(error)
}

pub fn provider_failure<T>(
    cx: &mut FunctionContext<'_>,
    name: &str,
    cause: Handle<'_, JsValue>,
) -> NeonResult<T> {
    let options = cx.empty_object();
    let _ = options.set(cx, "cause", cause)?;
    let message = cx.string(format!("Provider {name} failed.")).upcast();
    let error = construct(cx, "ProviderError", message, options.upcast())?;
    cx.throw(error)
}

pub fn register<'a>(cx: &mut ModuleContext<'a>, exports: Handle<'a, JsObject>) -> NeonResult<()> {
    let builtin = cx.global::<JsFunction>("Error")?;
    let mut base = builtin;
    for name in ["RiffError", "SchemaError", "GenerationError", "ProviderError"] {
        let constructor = JsFunction::with_name(cx, name, move |cx| constructor(cx, name))?;
        let base_prototype = base.get::<JsObject, _, _>(cx, "prototype")?;
        let prototype = constructor.get::<JsObject, _, _>(cx, "prototype")?;
        let _ =
            object_method(cx, "setPrototypeOf", &[prototype.upcast(), base_prototype.upcast()])?;
        let _ = object_method(cx, "setPrototypeOf", &[constructor.upcast(), base.upcast()])?;
        let _ = exports.set(cx, name, constructor)?;
        if name == "RiffError" {
            base = constructor;
        }
    }
    Ok(())
}
