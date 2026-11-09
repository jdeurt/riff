//! Bounded sampling profiles for the three complete input contracts.
use riff_core::{Error, Factory, GenerationOptions, ProviderContext, Result};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let required: Vec<_> = fields.iter().map(|(name, _)| *name).collect();
    let properties: Map<_, _> =
        fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

pub fn integer() -> Value {
    record([
        ("type", json!({"const":"integer"})),
        ("minimum", json!({"type":"integer","minimum":-30,"maximum":0})),
        ("maximum", json!({"type":"integer","minimum":1,"maximum":30})),
    ])
}

pub fn schema(family: u64) -> Value {
    match family {
        0 => integer(),
        1 => record([
            ("type", json!({"const":"string"})),
            ("minLength", json!({"type":"integer","minimum":1,"maximum":3})),
            ("maxLength", json!({"type":"integer","minimum":4,"maximum":7})),
            ("pattern", json!({"const":"^[A-C]+$"})),
        ]),
        2 => record([
            ("type", json!({"const":"object"})),
            ("required", json!({"const":["value"]})),
            ("properties", record([("value", integer())])),
            ("additionalProperties", json!({"type":"boolean"})),
        ]),
        3 => record([
            ("type", json!({"const":"array"})),
            ("minItems", json!({"const":2})),
            ("maxItems", json!({"const":4})),
            ("items", integer()),
            ("contains", json!({"const":{"const":0}})),
        ]),
        4 => record([(
            "oneOf",
            json!({"type":"array","minItems":2,"maxItems":2,"prefixItems":[integer(),{"const":{"const":"other"}}],"items":false}),
        )]),
        5 => record([
            ("type", json!({"const":"integer"})),
            (
                "allOf",
                json!({"type":"array","minItems":2,"maxItems":2,"prefixItems":[
                record([("minimum",json!({"type":"integer","minimum":-30,"maximum":0}))]),
                record([("maximum",json!({"type":"integer","minimum":1,"maximum":30}))])
            ],"items":false}),
            ),
        ]),
        6 => record([(
            "const",
            record([
                ("record", json!({"type":"integer","minimum":-30,"maximum":30})),
                ("flags", json!({"const":[true,false]})),
            ]),
        )]),
        _ => record([
            ("definitions", record([("Value", integer())])),
            ("type", json!({"const":"object"})),
            ("required", json!({"const":["value"]})),
            ("properties", json!({"const":{"value":{"$ref":"#/definitions/Value"}}})),
            ("additionalProperties", json!({"const":false})),
        ]),
    }
}

pub fn openapi() -> Value {
    let content = |body: Value| {
        record([("content", record([("application/json", record([("schema", body)]))]))])
    };
    let response = |values: Value| {
        record([
            ("description", json!({"type":"string","minLength":1,"maxLength":12})),
            (
                "content",
                record([(
                    "application/json",
                    record([("schema", json!({"const":{"type":"string","enum":values}}))]),
                )]),
            ),
        ])
    };
    record([
        ("openapi", json!({"enum":["3.0.3","3.1.0"]})),
        (
            "info",
            record([
                ("title", json!({"type":"string","minLength":1,"maxLength":12})),
                ("version", json!({"const":"1.0.0"})),
            ]),
        ),
        (
            "paths",
            record([(
                "/items",
                record([(
                    "post",
                    record([
                        ("requestBody", content(integer())),
                        (
                            "responses",
                            record([
                                ("201", response(json!(["created", "selected"]))),
                                ("400", response(json!(["rejected"]))),
                            ]),
                        ),
                    ]),
                )]),
            )]),
        ),
    ])
}

pub fn sidecar(strategy: &Value) -> Value {
    record([
        ("version", json!({"const":"0.1"})),
        (
            "fields",
            record([(
                "/properties/a~1b~0c",
                record([
                    ("strategy", json!({"const":strategy})),
                    (
                        "fail",
                        record([
                            (
                                "strategy",
                                json!({"const":{"kind":"schema","constraints":"violate"}}),
                            ),
                            ("reason", json!({"type":"string","minLength":1,"maxLength":24})),
                        ]),
                    ),
                ]),
            )]),
        ),
    ])
}

pub fn sample(profile: &Value, seed: u64) -> Result<Value> {
    let factory = Factory::new(profile, BTreeSet::new())?;
    let options = GenerationOptions { seed: Some(seed), ..GenerationOptions::default() };
    factory
        .generate(&options, &mut |_: &ProviderContext| {
            Err(Error::Provider("No custom provider in input profiles.".to_owned()))
        })?
        .pop()
        .ok_or_else(|| Error::Generation("Input profile produced no candidate.".to_owned()))
}

#[derive(Deserialize)]
pub struct Bounds {
    pub minimum: i64,
    pub maximum: i64,
}
