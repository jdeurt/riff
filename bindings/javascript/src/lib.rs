//! JavaScript values, callbacks, and errors translated for fixture operations.
mod errors;
mod factory;
mod json;
mod options;

use neon::prelude::*;
use neon::thread::LocalKey;
use riff_core::NumericMode;

// Constructors and model adapters belong to one JavaScript environment, never to a factory.
static API: LocalKey<Root<JsObject>> = LocalKey::new();

fn api<'a, C: Context<'a>>(cx: &mut C) -> JsResult<'a, JsObject> {
    if let Some(api) = API.get(cx) {
        return Ok(api.to_inner(cx));
    }
    cx.throw_error("Riff interface is not initialized.")
}

fn object_method<'a, C: Context<'a>>(
    cx: &mut C,
    name: &str,
    arguments: &[Handle<'a, JsValue>],
) -> JsResult<'a, JsValue> {
    let object = cx.global::<JsFunction>("Object")?;
    let method = object.get::<JsFunction, _, _>(cx, name)?;
    method.call(cx, object, arguments)
}

fn json_value<'a>(cx: &mut FunctionContext<'a>, source: &str) -> JsResult<'a, JsValue> {
    let object = cx.global::<JsObject>("JSON")?;
    let parse = object.get::<JsFunction, _, _>(cx, "parse")?;
    let argument = cx.string(source);
    parse.call(cx, object, [argument.upcast()])
}

fn fixture(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsValue>(0)?;
    let metadata = json::read(&mut cx, input, &mut Vec::new())?;
    if let Err(error) = riff_core::Metadata::parse(Some(&metadata)) {
        return errors::raise(&mut cx, error);
    }
    let payload = serde_json::json!({"x-fixture":metadata});
    json_value(&mut cx, &payload.to_string())
}

fn strip_metadata(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsValue>(0)?;
    let document = json::read(&mut cx, input, &mut Vec::new())?;
    json_value(&mut cx, &riff_core::strip_metadata(&document).to_string())
}

fn configure_models(mut cx: FunctionContext<'_>) -> JsResult<'_, JsUndefined> {
    let view = cx.argument::<JsFunction>(0)?;
    let schema = cx.argument::<JsFunction>(1)?;
    let exports = api(&mut cx)?;
    let _ = exports.set(&mut cx, "modelView", view)?;
    let _ = exports.set(&mut cx, "modelSchema", schema)?;
    Ok(cx.undefined())
}

fn cli(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let input = cx.argument::<JsValue>(0)?;
    let value = json::read(&mut cx, input, &mut Vec::new())?;
    let arguments: Vec<String> = match serde_json::from_str(&value.to_string()) {
        Ok(arguments) => arguments,
        Err(error) => return errors::raise(&mut cx, error.into()),
    };
    let output = match serde_json::to_string(&riff_core::run_cli(arguments, NumericMode::SafeFloat))
    {
        Ok(output) => output,
        Err(error) => return errors::raise(&mut cx, error.into()),
    };
    json_value(&mut cx, &output)
}

#[neon::main]
fn main(mut cx: ModuleContext<'_>) -> NeonResult<()> {
    let exports = cx.exports_object()?;
    errors::register(&mut cx, exports)?;
    factory::register(&mut cx, exports)?;
    cx.export_function("jsonValue", json::copy)?;
    cx.export_function("fixture", fixture)?;
    cx.export_function("stripFixtureMetadata", strip_metadata)?;
    cx.export_function("configureModels", configure_models)?;
    cx.export_function("cli", cli)?;
    let _ = API.get_or_try_init(&mut cx, |cx| Ok::<_, neon::result::Throw>(exports.root(cx)))?;
    Ok(())
}
