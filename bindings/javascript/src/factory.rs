//! JavaScript factory objects delegating complete operations to the shared session.
use std::collections::BTreeMap;

use neon::prelude::*;
use neon::types::JsBigInt;
use riff_core::{NumericMode, ProviderContext, Session};
use serde_json::{Value, json};

use crate::{api, errors, json as host_json, json_value, object_method, options};

struct NativeFactory {
    session: Session,
    provider_slots: BTreeMap<String, u32>,
}
impl Finalize for NativeFactory {}

fn readonly<'a, C: Context<'a>>(
    cx: &mut C,
    object: Handle<'a, JsObject>,
    key: &str,
    value: Handle<'a, JsValue>,
) -> NeonResult<()> {
    let descriptor = cx.empty_object();
    let _ = descriptor.set(cx, "value", value)?;
    let key = cx.string(key);
    let _ =
        object_method(cx, "defineProperty", &[object.upcast(), key.upcast(), descriptor.upcast()])?;
    Ok(())
}

fn factory_options<'a>(
    cx: &mut FunctionContext<'a>,
    source: &Value,
    options: Handle<'a, JsObject>,
    operation: Option<&Value>,
) -> JsResult<'a, JsObject> {
    let provider_input = options.get::<JsValue, _, _>(cx, "providers")?;
    let callbacks = JsArray::new(cx, 0);
    let mut provider_slots = BTreeMap::new();
    if !provider_input.is_a::<JsUndefined, _>(cx) {
        let provider_object = provider_input.downcast_or_throw::<JsObject, _>(cx)?;
        let entries = object_method(cx, "entries", &[provider_object.upcast()])?
            .downcast_or_throw::<JsArray, _>(cx)?;
        for index in 0..entries.len(cx) {
            let entry = entries.get::<JsArray, _, _>(cx, index)?;
            let key = entry.get::<JsString, _, _>(cx, 0)?;
            let name = host_json::text(cx, key)?;
            let callback = entry.get::<JsValue, _, _>(cx, 1)?;
            if !callback.is_a::<JsFunction, _>(cx) {
                return errors::raise(
                    cx,
                    riff_core::Error::Schema("Providers must be synchronous functions.".to_owned()),
                );
            }
            let _ = callbacks.set(cx, index, callback)?;
            let _ = provider_slots.insert(name, index);
        }
    }
    let fixture_input = options.get::<JsValue, _, _>(cx, "fixtures")?;
    let fixtures = if fixture_input.is_a::<JsUndefined, _>(cx) {
        None
    } else {
        Some(json!({"kind":"inline","value":host_json::read(cx,fixture_input,&mut Vec::new())?}))
    };
    let file_input = options.get::<JsValue, _, _>(cx, "fixtureFile")?;
    let fixture_file = if file_input.is_a::<JsUndefined, _>(cx) {
        Value::Null
    } else {
        host_json::read(cx, file_input, &mut Vec::new())?
    };
    let configuration = json!({"providers":provider_slots.keys().collect::<Vec<_>>(),"operation":operation,"fixtures":fixtures,"fixtureFile":fixture_file});
    let session =
        match Session::new(source.to_string(), configuration.to_string(), NumericMode::SafeFloat) {
            Ok(session) => session,
            Err(error) => return errors::raise(cx, error),
        };
    let factory = cx.empty_object();
    let state = cx.boxed(NativeFactory { session, provider_slots });
    readonly(cx, factory, "__riffState", state.upcast())?;
    readonly(cx, factory, "__riffProviders", callbacks.upcast())?;
    let _ = object_method(cx, "freeze", &[callbacks.upcast()])?;
    let constructor = api(cx)?.get::<JsFunction, _, _>(cx, "FixtureFactory")?;
    let prototype = constructor.get::<JsObject, _, _>(cx, "prototype")?;
    let _ = object_method(cx, "setPrototypeOf", &[factory.upcast(), prototype.upcast()])?;
    Ok(factory)
}

fn from_schema(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsValue>(0)?;
    let schema = host_json::read(&mut cx, input, &mut Vec::new())?;
    let options = options::object(&mut cx, 1)?;
    factory_options(&mut cx, &json!({"kind":"inline","value":schema}), options, None)
        .map(|object| object.upcast())
}
fn from_openapi(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsValue>(0)?;
    let document = host_json::read(&mut cx, input, &mut Vec::new())?;
    let selector = cx.argument::<JsValue>(1)?;
    let operation = host_json::read(&mut cx, selector, &mut Vec::new())?;
    let options = options::object(&mut cx, 2)?;
    factory_options(&mut cx, &json!({"kind":"inline","value":document}), options, Some(&operation))
        .map(|object| object.upcast())
}
fn from_file(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsString>(0)?;
    let path = host_json::text(&mut cx, input)?;
    let options = options::object(&mut cx, 1)?;
    let selector_path = options.get::<JsValue, _, _>(&mut cx, "path")?;
    let operation = if selector_path.is_a::<JsUndefined, _>(&mut cx) {
        None
    } else {
        let selector = cx.empty_object();
        for key in ["path", "method", "body", "status", "mediaType"] {
            let value = options.get::<JsValue, _, _>(&mut cx, key)?;
            if !value.is_a::<JsUndefined, _>(&mut cx) {
                let _ = selector.set(&mut cx, key, value)?;
            }
        }
        Some(host_json::read(&mut cx, selector.upcast(), &mut Vec::new())?)
    };
    factory_options(&mut cx, &json!({"kind":"file","value":path}), options, operation.as_ref())
        .map(|object| object.upcast())
}

fn for_model(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let model = cx.argument::<JsValue>(0)?;
    let options = options::object(&mut cx, 1)?;
    let exports = api(&mut cx)?;
    let schema = exports.get::<JsFunction, _, _>(&mut cx, "modelSchema")?;
    let receiver = cx.undefined();
    let schema = schema.call(&mut cx, receiver, [model])?;
    let schema = host_json::read(&mut cx, schema, &mut Vec::new())?;
    if !schema.is_object() {
        return errors::raise(
            &mut cx,
            riff_core::Error::Schema("Model must produce a schema object.".to_owned()),
        );
    }
    let factory =
        factory_options(&mut cx, &json!({"kind":"inline","value":schema}), options, None)?;
    let view = exports.get::<JsFunction, _, _>(&mut cx, "modelView")?;
    view.construct(&mut cx, [factory.upcast(), model]).map(|object| object.upcast())
}
fn as_model(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let receiver = cx.this::<JsObject>()?;
    let model = cx.argument::<JsValue>(0)?;
    let view = api(&mut cx)?.get::<JsFunction, _, _>(&mut cx, "modelView")?;
    view.construct(&mut cx, [receiver.upcast(), model]).map(|object| object.upcast())
}

fn provider_call<'a>(
    cx: &mut FunctionContext<'a>,
    callbacks: Handle<'a, JsArray>,
    slots: &BTreeMap<String, u32>,
    request: &ProviderContext,
    failure: &mut Option<neon::result::Throw>,
) -> riff_core::Result<Value> {
    let Some(slot) = slots.get(&request.name) else {
        return Err(riff_core::Error::Provider(format!(
            "Unknown registered provider: {}",
            request.name
        )));
    };
    let result = cx.try_catch(|cx| {
        let callback = callbacks.get::<JsFunction, _, _>(cx, *slot)?;
        let context = cx.empty_object();
        let seed = JsBigInt::from_u64(cx, request.seed);
        let locale = cx.string(&request.locale);
        let schema = json_value(cx, &request.schema.to_string())?;
        let provider_options = json_value(cx, &Value::Object(request.options.clone()).to_string())?;
        let _ = context.set(cx, "seed", seed)?;
        let _ = context.set(cx, "locale", locale)?;
        let _ = context.set(cx, "schema", schema)?;
        let _ = context.set(cx, "options", provider_options)?;
        let receiver = cx.undefined();
        let output = callback.call(cx, receiver, [context.upcast()])?;
        host_json::read(cx, output, &mut Vec::new())
    });
    match result {
        Ok(value) => Ok(value),
        Err(cause) => {
            if let Err(error) = errors::provider_failure::<()>(cx, &request.name, cause) {
                *failure = Some(error);
            }
            Err(riff_core::Error::Provider("Host provider failed.".to_owned()))
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
    let receiver = cx.this::<JsObject>()?;
    let factory = receiver.get::<JsBox<NativeFactory>, _, _>(&mut cx, "__riffState")?;
    let callbacks = receiver.get::<JsArray, _, _>(&mut cx, "__riffProviders")?;
    let input = options::object(&mut cx, 0)?;
    let controls = options::read(&mut cx, input, None)?.to_string();
    let mut failure = None;
    let result = {
        let mut provider = |request: &ProviderContext| {
            provider_call(&mut cx, callbacks, &factory.provider_slots, request, &mut failure)
        };
        match kind {
            Batch::Values => factory.session.generate(&controls, &mut provider),
            Batch::Success => factory.session.success_cases(&controls, &mut provider),
            Batch::Failures => factory.session.failure_cases(&controls, &mut provider),
            Batch::Cases => factory.session.cases(&controls, &mut provider),
        }
    };
    if let Some(error) = failure {
        return Err(error);
    }
    match result {
        Ok(output) => json_value(&mut cx, &output),
        Err(error) => errors::raise(&mut cx, error),
    }
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
    let receiver = cx.this::<JsObject>()?;
    let factory = receiver.get::<JsBox<NativeFactory>, _, _>(&mut cx, "__riffState")?;
    let input = cx.argument::<JsValue>(0)?;
    let value = host_json::read(&mut cx, input, &mut Vec::new())?;
    let input_options = options::object(&mut cx, 1)?;
    let oracle = input_options.get::<JsFunction, _, _>(&mut cx, "isFailure")?;
    let controls = options::read(&mut cx, input_options, Some("isFailure"))?.to_string();
    let mut failure = None;
    let result = factory.session.minimize(value.to_string(), controls, &mut |value| {
        let result = json_value(&mut cx, &value.to_string()).and_then(|argument| {
            oracle
                .call(&mut cx, input_options, [argument])
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
    let result = match result {
        Ok(output) => output,
        Err(error) => return errors::raise(&mut cx, error),
    };
    let result: riff_core::MinimizationResult = match serde_json::from_str(&result) {
        Ok(result) => result,
        Err(error) => return errors::raise(&mut cx, error.into()),
    };
    let output = json!({"value":result.value,"evaluations":result.evaluations,"budgetExhausted":result.budget_exhausted});
    json_value(&mut cx, &output.to_string())
}
fn is_valid(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let receiver = cx.this::<JsObject>()?;
    let factory = receiver.get::<JsBox<NativeFactory>, _, _>(&mut cx, "__riffState")?;
    let input = cx.argument::<JsValue>(0)?;
    let value = host_json::read(&mut cx, input, &mut Vec::new())?;
    match factory.session.is_valid(value.to_string()) {
        Ok(valid) => Ok(cx.boolean(valid).upcast()),
        Err(error) => errors::raise(&mut cx, error),
    }
}

fn constructor(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    cx.throw_type_error("Use FixtureFactory.fromSchema, fromOpenAPI, fromFile, or forModel.")
}

type Method = for<'a> fn(FunctionContext<'a>) -> JsResult<'a, JsValue>;
pub fn register<'a>(cx: &mut ModuleContext<'a>, exports: Handle<'a, JsObject>) -> NeonResult<()> {
    let constructor = JsFunction::with_name(cx, "FixtureFactory", constructor)?;
    let prototype = constructor.get::<JsObject, _, _>(cx, "prototype")?;
    let methods: &[(&str, Method)] = &[
        ("generate", generate),
        ("successCases", success_cases),
        ("failureCases", failure_cases),
        ("generateCases", cases),
        ("minimize", minimize),
        ("isValid", is_valid),
        ("asModel", as_model),
    ];
    for (name, method) in methods {
        let function = JsFunction::with_name(cx, name, *method)?;
        readonly(cx, prototype, name, function.upcast())?;
    }
    let methods: &[(&str, Method)] = &[
        ("fromSchema", from_schema),
        ("fromOpenAPI", from_openapi),
        ("fromFile", from_file),
        ("forModel", for_model),
    ];
    for (name, method) in methods {
        let function = JsFunction::with_name(cx, name, *method)?;
        let _ = constructor.set(cx, *name, function)?;
    }
    let _ = object_method(cx, "freeze", &[prototype.upcast()])?;
    let _ = exports.set(cx, "FixtureFactory", constructor)?;
    Ok(())
}
