//! Seeded fuzzing of the complete JSON Schema, `OpenAPI`, and sidecar input contracts.
//! Full contracts are independent oracles; bounded profiles let Riff generate candidate inputs.
use std::collections::BTreeSet;

use jsonschema::{Registry, Validator};
use riff_core::{
    Body, Error, Factory, GenerationOptions, Operation, ProviderContext, Result, prepare,
};
use serde_json::{Map, Value, json};

#[path = "support/input_profiles.rs"]
mod profiles;

struct Contracts {
    schema: Validator,
    openapi: Validator,
    sidecar: Validator,
}

impl Contracts {
    fn load() -> Result<Self> {
        let schema: Value = serde_json::from_str(include_str!("../schemas/json-schema.json"))?;
        let openapi: Value = serde_json::from_str(include_str!("../schemas/openapi.json"))?;
        let sidecar: Value = serde_json::from_str(include_str!("../schemas/sidecar.json"))?;
        let registry = Registry::new()
            .retrieve_known_draft_meta_schemas(false)
            .add("https://riff.invalid/inputs/json-schema", &schema)
            .and_then(|builder| builder.add("https://riff.invalid/inputs/openapi", &openapi))
            .and_then(|builder| builder.add("https://riff.invalid/inputs/sidecar", &sidecar))
            .and_then(jsonschema::RegistryBuilder::prepare)
            .map_err(|error| Error::Schema(error.to_string()))?;
        let build = |value: &Value| {
            jsonschema::options()
                .with_registry(&registry)
                .build(value)
                .map_err(|error| Error::Schema(error.to_string()))
        };
        Ok(Self { schema: build(&schema)?, openapi: build(&openapi)?, sidecar: build(&sidecar)? })
    }
}

#[derive(Clone)]
enum Expected {
    Integer { minimum: i64, maximum: i64 },
    Text { minimum: usize, maximum: usize },
    Object { minimum: i64, maximum: i64, closed: bool },
    Array { minimum: i64, maximum: i64 },
    Alternative { minimum: i64, maximum: i64 },
    Constant(Value),
}

fn assert_success(expected: &Expected, value: &Value) {
    match expected {
        Expected::Integer { minimum, maximum } => {
            assert!(value.as_i64().is_some_and(|number| (*minimum..=*maximum).contains(&number)));
        }
        Expected::Text { minimum, maximum } => {
            assert!(value.as_str().is_some_and(|text| (*minimum..=*maximum).contains(&text.len())
                && text.bytes().all(|byte| matches!(byte, b'A'..=b'C'))));
        }
        Expected::Object { minimum, maximum, closed } => {
            let Some(object) = value.as_object() else {
                assert!(value.is_object(), "Expected object: {value}");
                return;
            };
            assert!(
                object
                    .get("value")
                    .and_then(Value::as_i64)
                    .is_some_and(|number| (*minimum..=*maximum).contains(&number))
            );
            if *closed {
                assert_eq!(object.len(), 1);
            }
        }
        Expected::Array { minimum, maximum } => {
            let Some(values) = value.as_array() else {
                assert!(value.is_array(), "Expected array: {value}");
                return;
            };
            assert!((2..=4).contains(&values.len()));
            assert!(values.iter().all(|value| {
                value.as_i64().is_some_and(|number| (*minimum..=*maximum).contains(&number))
            }));
            assert!(values.iter().any(|value| value.as_i64() == Some(0)));
        }
        Expected::Alternative { minimum, maximum } => assert!(
            value == "other"
                || value.as_i64().is_some_and(|number| (*minimum..=*maximum).contains(&number))
        ),
        Expected::Constant(expected) => assert_eq!(value, expected),
    }
}

fn custom_value(context: &ProviderContext) -> Result<Value> {
    context
        .options
        .get("value")
        .cloned()
        .ok_or_else(|| Error::Provider("Missing case value.".to_owned()))
}

fn options(seed: u64) -> GenerationOptions {
    GenerationOptions { count: 4, seed: Some(seed), ..GenerationOptions::default() }
}

fn candidate(family: u64, seed: u64) -> Result<(Value, Expected, String)> {
    let schema = profiles::sample(&profiles::schema(family), seed)?;
    let numeric = match family {
        0 => schema.clone(),
        2 => schema.pointer("/properties/value").cloned().unwrap_or(Value::Null),
        3 => schema.get("items").cloned().unwrap_or(Value::Null),
        4 => schema.pointer("/oneOf/0").cloned().unwrap_or(Value::Null),
        5 => {
            json!({"minimum":schema.pointer("/allOf/0/minimum"),"maximum":schema.pointer("/allOf/1/maximum")})
        }
        7 => schema.pointer("/definitions/Value").cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let expected = match family {
        1 => Expected::Text {
            minimum: serde_json::from_value(
                schema.get("minLength").cloned().unwrap_or(Value::Null),
            )?,
            maximum: serde_json::from_value(
                schema.get("maxLength").cloned().unwrap_or(Value::Null),
            )?,
        },
        6 => Expected::Constant(schema.get("const").cloned().unwrap_or(Value::Null)),
        _ => {
            let bounds: profiles::Bounds = serde_json::from_value(numeric)?;
            assert!((-30..=0).contains(&bounds.minimum) && (1..=30).contains(&bounds.maximum));
            match family {
                2 | 7 => Expected::Object {
                    minimum: bounds.minimum,
                    maximum: bounds.maximum,
                    closed: schema.get("additionalProperties") == Some(&Value::Bool(false)),
                },
                3 => Expected::Array { minimum: bounds.minimum, maximum: bounds.maximum },
                4 => Expected::Alternative { minimum: bounds.minimum, maximum: bounds.maximum },
                _ => Expected::Integer { minimum: bounds.minimum, maximum: bounds.maximum },
            }
        }
    };
    let pointer = match family {
        2 | 7 => "/properties/value",
        3 => "/items",
        _ => "",
    };
    Ok((schema, expected, pointer.to_owned()))
}

#[test]
fn fuzz_json_schema_inputs_and_sidecar_mutations() -> Result<()> {
    let contracts = Contracts::load()?;
    let dialects = [
        "http://json-schema.org/draft-07/schema#",
        "https://json-schema.org/draft/2019-09/schema",
        "https://json-schema.org/draft/2020-12/schema",
    ];
    for seed in 0..96_u64 {
        let family = seed % 8;
        let (mut schema, expected, pointer) = candidate(family, seed)?;
        let dialect = dialects
            .get(usize::try_from(seed % 3).map_err(|error| Error::Schema(error.to_string()))?)
            .ok_or_else(|| Error::Schema("Missing test dialect.".to_owned()))?;
        let Some(object) = schema.as_object_mut() else {
            return Err(Error::Schema("Expected schema object.".to_owned()));
        };
        let _ = object.insert("$schema".to_owned(), Value::String((*dialect).to_owned()));
        assert!(contracts.schema.is_valid(&schema), "Invalid generated schema: {schema}");
        let mut fields = Map::new();
        let _ = fields.insert(pointer, json!({"fail":{"examples":[false]}}));
        let sidecar = json!({"version":"0.1","fields":fields});
        assert!(contracts.sidecar.is_valid(&sidecar));
        let mut invalid = schema.clone();
        let (keyword, replacement) = match seed % 6 {
            0 => ("type", json!(17)),
            1 => ("minLength", json!(-1)),
            2 => ("multipleOf", json!(0)),
            3 => ("required", json!("value")),
            4 => ("additionalProperties", json!("closed")),
            _ => ("allOf", json!([17])),
        };
        let object = invalid
            .as_object_mut()
            .ok_or_else(|| Error::Schema("Missing sampled schema.".to_owned()))?;
        let _ = object.insert(keyword.to_owned(), replacement);
        assert!(!contracts.schema.is_valid(&invalid));
        assert!(matches!(Factory::new(&invalid, BTreeSet::new()), Err(Error::Schema(_))));
        let normalized = prepare(&schema, None, Some(&sidecar))?;
        let factory = Factory::new(&normalized, BTreeSet::new())?;
        let generated = factory.generate(&options(seed), &mut custom_value)?;
        assert_eq!(generated.len(), 4);
        for value in generated {
            assert_success(&expected, &value);
        }
        let cases = factory.failure_cases(&options(seed), &mut custom_value)?;
        assert_eq!(cases.len(), 4);
        for case in cases {
            let target = case
                .target
                .as_deref()
                .ok_or_else(|| Error::Schema("Missing failure target.".to_owned()))?;
            match family {
                2 | 7 => assert_eq!(target, "/value"),
                3 => {
                    let values = case
                        .value
                        .as_array()
                        .ok_or_else(|| Error::Schema("Expected failed array.".to_owned()))?;
                    let index =
                        target.strip_prefix('/').and_then(|index| index.parse::<usize>().ok());
                    assert!(index.is_some_and(|index| index < values.len()));
                }
                _ => assert_eq!(target, ""),
            }
            assert_eq!(case.value.pointer(target), Some(&Value::Bool(false)));
            assert!(!case.schema_valid);
            assert!(!case.violations.is_empty());
        }
    }
    Ok(())
}

#[test]
fn fuzz_openapi_inputs_with_exact_request_response_selection() -> Result<()> {
    let contracts = Contracts::load()?;
    for seed in 0..48_u64 {
        let document = profiles::sample(&profiles::openapi(), seed)?;
        let bounds: profiles::Bounds = serde_json::from_value(
            document
                .pointer("/paths/~1items/post/requestBody/content/application~1json/schema")
                .cloned()
                .unwrap_or(Value::Null),
        )?;
        let minimum = bounds.minimum;
        assert!(contracts.openapi.is_valid(&document), "Invalid generated OpenAPI: {document}");
        let sidecar = json!({"version":"0.1","fields":{"":{"strategy":{"kind":"values","values":["wrong global"]}}},"operations":[
            {"path":"/items","method":"post","body":"request","fields":{"":{"strategy":{"kind":"values","values":[minimum]},"fail":{"examples":["bad request"]}}}},
            {"path":"/items","method":"post","body":"response","status":"201","fields":{"":{"strategy":{"kind":"values","values":["selected"]},"fail":{"examples":[false]}}}}
        ]});
        assert!(contracts.sidecar.is_valid(&sidecar));
        for (body, expected) in
            [(Body::Request, json!(minimum)), (Body::Response, json!("selected"))]
        {
            let operation = Operation {
                path: "/items".to_owned(),
                method: "POST".to_owned(),
                body,
                status: "201".to_owned(),
                media_type: "application/json".to_owned(),
            };
            let schema = prepare(&document, Some(&operation), Some(&sidecar))?;
            assert!(contracts.schema.is_valid(&schema));
            let factory = Factory::new(&schema, BTreeSet::new())?;
            assert_eq!(factory.generate(&options(seed), &mut custom_value)?, vec![expected; 4]);
            let cases = factory.failure_cases(&options(seed), &mut custom_value)?;
            assert_eq!(cases.len(), 4);
            for case in cases {
                assert_eq!(case.target.as_deref(), Some(""));
                assert_eq!(
                    case.value,
                    if body == Body::Request { json!("bad request") } else { json!(false) }
                );
                assert!(!case.schema_valid);
            }
        }
        let missing = Operation {
            path: "/missing".to_owned(),
            method: "post".to_owned(),
            body: Body::Request,
            status: "200".to_owned(),
            media_type: "application/json".to_owned(),
        };
        assert!(matches!(prepare(&document, Some(&missing), None), Err(Error::Schema(_))));
        let valid_operation = Operation { path: "/items".to_owned(), ..missing };
        let mut unsupported = document.clone();
        let object = unsupported
            .as_object_mut()
            .ok_or_else(|| Error::Schema("Missing sampled document.".to_owned()))?;
        let _ = object.insert("openapi".to_owned(), Value::String(format!("2.{seed}")));
        assert!(!contracts.openapi.is_valid(&unsupported));
        assert!(matches!(
            prepare(&unsupported, Some(&valid_operation), None),
            Err(Error::Schema(_))
        ));
    }
    Ok(())
}

#[test]
fn fuzz_full_sidecar_strategy_inputs_and_reject_invalid_annotations() -> Result<()> {
    let contracts = Contracts::load()?;
    for seed in 0..60_u64 {
        let schema = json!({"type":"object","required":["a/b~c"],"properties":{"a/b~c":{"type":"integer","minimum":1,"maximum":50}},"additionalProperties":false});
        let strategy = match seed % 5 {
            0 => json!({"kind":"schema","constraints":"respect"}),
            1 => json!({"kind":"values","values":[7,50]}),
            2 => {
                json!({"kind":"provider","name":"case.value","locale":"sv-SE","options":{"value":7}})
            }
            3 => {
                json!({"kind":"one-of","strategies":[{"kind":"values","values":[7]},{"kind":"schema"}]})
            }
            _ => {
                json!({"kind":"filter","schema":{"minimum":10},"strategy":{"kind":"values","values":[0,7,50]}})
            }
        };
        let sidecar = profiles::sample(&profiles::sidecar(&strategy), seed)?;
        assert!(contracts.sidecar.is_valid(&sidecar), "Invalid generated sidecar: {sidecar}");
        let reason = sidecar
            .pointer("/fields/~1properties~1a~01b~00c/fail/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Schema("Missing sampled failure reason.".to_owned()))?;
        let normalized = prepare(&schema, None, Some(&sidecar))?;
        let factory = Factory::new(&normalized, BTreeSet::from(["case.value".to_owned()]))?;
        for value in factory.generate(&options(seed), &mut custom_value)? {
            assert!(
                value
                    .get("a/b~c")
                    .and_then(Value::as_i64)
                    .is_some_and(|number| (1..=50).contains(&number))
            );
            if seed % 5 == 2 {
                assert_eq!(value, json!({"a/b~c":7}));
            }
            if seed % 5 == 4 {
                assert_eq!(value, json!({"a/b~c":50}));
            }
        }
        let cases = factory.failure_cases(&options(seed), &mut custom_value)?;
        assert_eq!(cases.len(), 4);
        for case in cases {
            assert_eq!(case.target.as_deref(), Some("/a~1b~0c"));
            assert_eq!(case.reason.as_deref(), Some(reason));
            assert!(
                case.value
                    .get("a/b~c")
                    .and_then(Value::as_i64)
                    .is_some_and(|number| !(1..=50).contains(&number))
            );
        }
        let invalid_metadata = match seed % 4 {
            0 => json!({"strategy":{"kind":"schema","constraints":"violate"}}),
            1 => json!({"provider":"case.value","strategy":{"kind":"values","values":[7]}}),
            2 => json!({"fail":{}}),
            _ => json!({"locale":"en-US"}),
        };
        let invalid_sidecar =
            json!({"version":"0.1","fields":{"/properties/a~1b~0c":invalid_metadata}});
        assert!(!contracts.sidecar.is_valid(&invalid_sidecar));
        assert!(matches!(prepare(&schema, None, Some(&invalid_sidecar)), Err(Error::Schema(_))));
    }
    Ok(())
}

#[test]
fn fuzz_boolean_schema_inputs_with_known_satisfiability() -> Result<()> {
    let contracts = Contracts::load()?;
    for seed in 0..16_u64 {
        let schema = profiles::sample(&json!({"enum":[true,false]}), seed)?;
        assert!(contracts.schema.is_valid(&schema));
        let factory = Factory::new(&schema, BTreeSet::new())?;
        if schema == true {
            assert_eq!(factory.generate(&options(seed), &mut custom_value)?.len(), 4);
        } else {
            assert_eq!(schema, false);
            assert!(matches!(
                factory.generate(&options(seed), &mut custom_value),
                Err(Error::Generation(_))
            ));
        }
    }
    Ok(())
}
