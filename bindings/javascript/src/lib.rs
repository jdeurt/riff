//! JavaScript values, callbacks, and errors translated for fixture operations.
mod json;

use neon::prelude::*;
use neon::types::{JsBigInt, Value as _};
use riff_core::{NumericMode, ProviderContext, Session};
use serde_json::Value;

struct NativeFactory(Session);
impl Finalize for NativeFactory {}

fn throw_core<T>(cx: &mut FunctionContext<'_>, error: riff_core::Error) -> NeonResult<T> {
    let (name, message) = match error {
        riff_core::Error::Argument(message) => return cx.throw_range_error(message),
        riff_core::Error::Schema(message) => ("SchemaError", message),
        riff_core::Error::Generation(message) => ("GenerationError", message),
        riff_core::Error::Provider(message) => ("ProviderError", message),
    };
    let object = cx.error(message)?;
    let name = cx.string(name);
    let _ = object.set(cx, "name", name)?;
    cx.throw(object)
}

fn json_value<'a>(cx: &mut FunctionContext<'a>, source: &str) -> JsResult<'a, JsValue> {
    let object = cx.global::<JsObject>("JSON")?;
    let parse = object.get::<JsFunction, _, _>(cx, "parse")?;
    let argument = cx.string(source);
    parse.call(cx, object, [argument.upcast()])
}

fn compile(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBox<NativeFactory>> {
    let source = cx.argument::<JsString>(0)?.value(&mut cx);
    let configuration = cx.argument::<JsString>(1)?.value(&mut cx);
    match Session::new(&source, &configuration, NumericMode::SafeFloat) {
        Ok(session) => Ok(cx.boxed(NativeFactory(session))),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn provider_call(
    cx: &mut FunctionContext<'_>,
    callback: Handle<'_, JsFunction>,
    request: &ProviderContext,
    failure: &mut Option<neon::result::Throw>,
) -> riff_core::Result<Value> {
    let serialized = serde_json::to_string(request)?;
    let result = json_value(cx, &serialized).and_then(|request| {
        let receiver = cx.undefined();
        callback
            .call(cx, receiver, [request])
            .and_then(|value| value.downcast_or_throw::<JsString, _>(cx))
    });
    match result {
        Ok(output) => serde_json::from_str(&output.value(cx))
            .map_err(|error| riff_core::Error::Provider(error.to_string())),
        Err(error) => {
            *failure = Some(error);
            Err(riff_core::Error::Provider("Host provider raised.".to_owned()))
        }
    }
}

#[derive(Clone, Copy)]
enum Batch {
    Values,
    Success,
    Failures,
    Cases,
}

fn batch(mut cx: FunctionContext<'_>, kind: Batch) -> JsResult<'_, JsValue> {
    let factory = cx.argument::<JsBox<NativeFactory>>(0)?;
    let options = cx.argument::<JsString>(1)?.value(&mut cx);
    let callback = cx.argument::<JsFunction>(2)?;
    let mut failure = None;
    let result = {
        let mut provider =
            |request: &ProviderContext| provider_call(&mut cx, callback, request, &mut failure);
        match kind {
            Batch::Values => factory.0.generate(&options, &mut provider),
            Batch::Success => factory.0.success_cases(&options, &mut provider),
            Batch::Failures => factory.0.failure_cases(&options, &mut provider),
            Batch::Cases => factory.0.cases(&options, &mut provider),
        }
    };
    if let Some(error) = failure {
        return Err(error);
    }
    match result {
        Ok(output) => json_value(&mut cx, &output),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn option_value(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let value = cx.argument::<JsValue>(1)?;
    if value.is_a::<JsBigInt, _>(&mut cx) {
        return value.to_string(&mut cx).map(|value| value.upcast());
    }
    if let Ok(number) = value.downcast::<JsNumber, _>(&mut cx)
        && !number.value(&mut cx).is_finite()
    {
        return cx.throw_range_error("Invocation numbers must be finite.");
    }
    Ok(value)
}

fn encode_options(mut cx: FunctionContext<'_>) -> JsResult<'_, JsString> {
    let value = cx.argument::<JsValue>(0)?;
    let json = cx.global::<JsObject>("JSON")?;
    let stringify = json.get::<JsFunction, _, _>(&mut cx, "stringify")?;
    let replacer = JsFunction::new(&mut cx, option_value)?;
    stringify
        .call(&mut cx, json, [value, replacer.upcast()])?
        .downcast_or_throw::<JsString, _>(&mut cx)
}

fn generate(cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    batch(cx, Batch::Values)
}
fn success_cases(cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    batch(cx, Batch::Success)
}
fn failure_cases(cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    batch(cx, Batch::Failures)
}
fn cases(cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    batch(cx, Batch::Cases)
}

fn minimize(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let factory = cx.argument::<JsBox<NativeFactory>>(0)?;
    let value = cx.argument::<JsString>(1)?.value(&mut cx);
    let options = cx.argument::<JsString>(2)?.value(&mut cx);
    let callback = cx.argument::<JsFunction>(3)?;
    let mut failure = None;
    let result = factory.0.minimize(&value, &options, &mut |value| {
        let payload = serde_json::to_string(value)?;
        let result = json_value(&mut cx, &payload).and_then(|argument| {
            let receiver = cx.undefined();
            callback
                .call(&mut cx, receiver, [argument])
                .and_then(|value| value.downcast_or_throw::<JsBoolean, _>(&mut cx))
        });
        match result {
            Ok(value) => Ok(value.value(&mut cx)),
            Err(error) => {
                failure = Some(error);
                Err(riff_core::Error::Provider("Oracle raised.".to_owned()))
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    match result {
        Ok(output) => json_value(&mut cx, &output),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn is_valid(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBoolean> {
    let factory = cx.argument::<JsBox<NativeFactory>>(0)?;
    let payload = cx.argument::<JsString>(1)?.value(&mut cx);
    match factory.0.is_valid(&payload) {
        Ok(valid) => Ok(cx.boolean(valid)),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn strip_metadata(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let payload = cx.argument::<JsString>(0)?.value(&mut cx);
    let result = serde_json::from_str::<Value>(&payload)
        .map(|value| riff_core::strip_metadata(&value))
        .and_then(|value| serde_json::to_string(&value));
    match result {
        Ok(output) => json_value(&mut cx, &output),
        Err(error) => throw_core(&mut cx, error.into()),
    }
}

fn validate_metadata(mut cx: FunctionContext<'_>) -> JsResult<'_, JsUndefined> {
    let payload = cx.argument::<JsString>(0)?.value(&mut cx);
    let result = serde_json::from_str::<Value>(&payload)
        .map_err(Into::into)
        .and_then(|value| riff_core::Metadata::parse(Some(&value)));
    match result {
        Ok(_) => Ok(cx.undefined()),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn cli(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let payload = cx.argument::<JsString>(0)?.value(&mut cx);
    let arguments: Vec<String> = match serde_json::from_str(&payload) {
        Ok(arguments) => arguments,
        Err(error) => return throw_core(&mut cx, error.into()),
    };
    match serde_json::to_string(&riff_core::run_cli(arguments, NumericMode::SafeFloat)) {
        Ok(output) => json_value(&mut cx, &output),
        Err(error) => throw_core(&mut cx, error.into()),
    }
}

#[neon::main]
fn main(mut cx: ModuleContext<'_>) -> NeonResult<()> {
    cx.export_function("jsonValue", json::copy)?;
    cx.export_function("encodeOptions", encode_options)?;
    cx.export_function("compile", compile)?;
    cx.export_function("generate", generate)?;
    cx.export_function("cases", cases)?;
    cx.export_function("successCases", success_cases)?;
    cx.export_function("failureCases", failure_cases)?;
    cx.export_function("minimize", minimize)?;
    cx.export_function("isValid", is_valid)?;
    cx.export_function("stripMetadata", strip_metadata)?;
    cx.export_function("validateMetadata", validate_metadata)?;
    cx.export_function("cli", cli)?;
    Ok(())
}
