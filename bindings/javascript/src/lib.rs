//! Neon binding for the shared Riff engine.
use std::collections::BTreeSet;

use neon::prelude::*;
use riff_core::{Factory, GenerationOptions, Operation, ProviderContext};
use serde::Deserialize;
use serde_json::Value;

struct NativeFactory(Factory);
impl Finalize for NativeFactory {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    providers: BTreeSet<String>,
    locales: BTreeSet<String>,
    operation: Option<Operation>,
    fixtures: Option<Value>,
}

fn throw_core<T>(cx: &mut FunctionContext<'_>, error: riff_core::Error) -> NeonResult<T> {
    let (name, message) = match error {
        riff_core::Error::Schema(message) => ("SchemaError", message),
        riff_core::Error::Generation(message) => ("GenerationError", message),
        riff_core::Error::Provider(message) => ("ProviderError", message),
    };
    let object = cx.error(message)?;
    let name = cx.string(name);
    let _ = object.set(cx, "name", name)?;
    cx.throw(object)
}

fn compile(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBox<NativeFactory>> {
    let document = cx.argument::<JsString>(0)?.value(&mut cx);
    let configuration = cx.argument::<JsString>(1)?.value(&mut cx);
    let result = (|| -> riff_core::Result<Factory> {
        let document: Value = serde_json::from_str(&document)?;
        let configuration: Configuration = serde_json::from_str(&configuration)?;
        let schema = riff_core::prepare(
            &document,
            configuration.operation.as_ref(),
            configuration.fixtures.as_ref(),
        )?;
        Factory::new(&schema, configuration.providers, configuration.locales)
    })();
    match result {
        Ok(factory) => Ok(cx.boxed(NativeFactory(factory))),
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
    let request = cx.string(serialized);
    let receiver = cx.undefined();
    match callback
        .call(cx, receiver, [request.upcast()])
        .and_then(|value| value.downcast_or_throw::<JsString, _>(cx))
    {
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
    Failures,
}

fn batch(mut cx: FunctionContext<'_>, kind: Batch) -> JsResult<'_, JsString> {
    let factory = cx.argument::<JsBox<NativeFactory>>(0)?;
    let options = cx.argument::<JsString>(1)?.value(&mut cx);
    let callback = cx.argument::<JsFunction>(2)?;
    let options: GenerationOptions = match serde_json::from_str(&options) {
        Ok(options) => options,
        Err(error) => return throw_core(&mut cx, error.into()),
    };
    let mut failure = None;
    let result = {
        let mut provider =
            |request: &ProviderContext| provider_call(&mut cx, callback, request, &mut failure);
        match kind {
            Batch::Values => factory
                .0
                .generate(&options, &mut provider)
                .and_then(|values| serde_json::to_string(&values).map_err(Into::into)),
            Batch::Failures => factory
                .0
                .failure_cases(&options, &mut provider)
                .and_then(|values| serde_json::to_string(&values).map_err(Into::into)),
        }
    };
    if let Some(error) = failure {
        return Err(error);
    }
    match result {
        Ok(output) => Ok(cx.string(output)),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn generate(cx: FunctionContext<'_>) -> JsResult<'_, JsString> {
    batch(cx, Batch::Values)
}
fn failure_cases(cx: FunctionContext<'_>) -> JsResult<'_, JsString> {
    batch(cx, Batch::Failures)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MinimizeOptions {
    max_evaluations: usize,
    preserve_schema: bool,
}

fn minimize(mut cx: FunctionContext<'_>) -> JsResult<'_, JsString> {
    let factory = cx.argument::<JsBox<NativeFactory>>(0)?;
    let value = cx.argument::<JsString>(1)?.value(&mut cx);
    let options = cx.argument::<JsString>(2)?.value(&mut cx);
    let callback = cx.argument::<JsFunction>(3)?;
    let input: Value = match serde_json::from_str(&value) {
        Ok(input) => input,
        Err(error) => return throw_core(&mut cx, error.into()),
    };
    let options: MinimizeOptions = match serde_json::from_str(&options) {
        Ok(options) => options,
        Err(error) => return throw_core(&mut cx, error.into()),
    };
    let mut failure = None;
    let result = factory.0.minimize(
        &input,
        options.max_evaluations,
        options.preserve_schema,
        &mut |value| {
            let payload = serde_json::to_string(value)?;
            let argument = cx.string(payload);
            let receiver = cx.undefined();
            match callback
                .call(&mut cx, receiver, [argument.upcast()])
                .and_then(|value| value.downcast_or_throw::<JsBoolean, _>(&mut cx))
            {
                Ok(value) => Ok(value.value(&mut cx)),
                Err(error) => {
                    failure = Some(error);
                    Err(riff_core::Error::Provider("Oracle raised.".to_owned()))
                }
            }
        },
    );
    if let Some(error) = failure {
        return Err(error);
    }
    match result.and_then(|value| serde_json::to_string(&value).map_err(Into::into)) {
        Ok(output) => Ok(cx.string(output)),
        Err(error) => throw_core(&mut cx, error),
    }
}

fn is_valid(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBoolean> {
    let factory = cx.argument::<JsBox<NativeFactory>>(0)?;
    let payload = cx.argument::<JsString>(1)?.value(&mut cx);
    let input: Value = match serde_json::from_str(&payload) {
        Ok(input) => input,
        Err(error) => return throw_core(&mut cx, error.into()),
    };
    Ok(cx.boolean(factory.0.is_valid(&input)))
}

fn strip_metadata(mut cx: FunctionContext<'_>) -> JsResult<'_, JsString> {
    let payload = cx.argument::<JsString>(0)?.value(&mut cx);
    let result = serde_json::from_str::<Value>(&payload)
        .map(|value| riff_core::strip_metadata(&value))
        .and_then(|value| serde_json::to_string(&value));
    match result {
        Ok(output) => Ok(cx.string(output)),
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

#[neon::main]
fn main(mut cx: ModuleContext<'_>) -> NeonResult<()> {
    cx.export_function("compile", compile)?;
    cx.export_function("generate", generate)?;
    cx.export_function("failureCases", failure_cases)?;
    cx.export_function("minimize", minimize)?;
    cx.export_function("isValid", is_valid)?;
    cx.export_function("stripMetadata", strip_metadata)?;
    cx.export_function("validateMetadata", validate_metadata)?;
    Ok(())
}
