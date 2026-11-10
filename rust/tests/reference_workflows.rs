//! Public generation workflows for scoped and recursive input contracts.
use std::collections::BTreeSet;

use riff_core::{Error, Factory, GenerationOptions, NumericMode, Profile, Result, Session};
use serde_json::{Value, json};

#[test]
fn inactive_vocabulary_and_later_formats_do_not_constrain_older_dialects() -> Result<()> {
    for schema in [
        json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"integer","enum":[7],"const":1,"if":true,"then":false,"contains":false,"propertyNames":false}),
        json!({"$schema":"http://json-schema.org/draft-07/schema#","type":"integer","const":7,"prefixItems":[{"$ref":"#/missing"}],"$dynamicRef":"#/missing"}),
        json!({"$schema":"https://json-schema.org/draft/2019-09/schema","type":"integer","const":7,"dependencies":{"ignored":{"$ref":"#/missing"}}}),
    ] {
        let factory = Factory::new(&schema)?;
        let options =
            GenerationOptions { count: 10, seed: Some(42), ..GenerationOptions::default() };
        assert_eq!(factory.generate(&options)?, vec![json!(7); 10]);
        assert!(factory.is_valid(&json!(7)));
    }
    let dependencies = json!({"$schema":"https://json-schema.org/draft/2019-09/schema","type":"object","properties":{"a":{"const":1},"b":{"const":2}},"required":["a"],"additionalProperties":false,"dependentRequired":{"a":["b"]},"dependencies":{"a":["ignored"]}});
    assert_eq!(
        Factory::new(&dependencies)?.generate(&GenerationOptions {
            count: 10,
            seed: Some(42),
            ..GenerationOptions::default()
        })?,
        vec![json!({"a":1,"b":2}); 10]
    );
    let hostname = Factory::new(
        &json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"string","format":"hostname","enum":["ab--cd.example"]}),
    )?;
    assert!(hostname.is_valid(&json!("ab--cd.example")));
    assert_eq!(hostname.generate(&GenerationOptions::default())?, vec![json!("ab--cd.example")]);
    let schema = json!({"$schema":"http://json-schema.org/draft-07/schema#","type":"string","format":"uuid","maxLength":1});
    let values = Factory::new(&schema)?.generate(&GenerationOptions {
        count: 20,
        seed: Some(42),
        ..GenerationOptions::default()
    })?;
    assert!(
        values.iter().all(|value| value.as_str().is_some_and(|text| text.chars().count() <= 1))
    );
    Ok(())
}

#[test]
fn relative_resource_ids_and_static_anchors_keep_their_declared_targets() -> Result<()> {
    let options = GenerationOptions { count: 10, seed: Some(42), ..GenerationOptions::default() };
    for link in ["node/", "#/$defs/Node"] {
        let schema = json!({"$id":"https://example.test/root/","$ref":link,"$defs":{"Node":{"$id":"node/","$ref":"leaf","$defs":{"Leaf":{"$id":"leaf","type":"integer","const":7}}}}});
        assert_eq!(Factory::new(&schema)?.generate(&options)?, vec![json!(7); 10]);
    }
    let schema = json!({"$id":"https://example.test/outer","$dynamicAnchor":"node","type":"object","required":["child"],"properties":{"child":{"$ref":"https://example.test/inner#node"}},"additionalProperties":false,"$defs":{"Inner":{"$id":"https://example.test/inner","$dynamicAnchor":"node","type":"integer","const":7}}});
    let factory = Factory::new(&schema)?;
    assert!(factory.is_valid(&json!({"child":7})));
    assert!(!factory.is_valid(&json!({"child":{}})));
    assert_eq!(factory.generate(&options)?, vec![json!({"child":7}); 10]);
    Ok(())
}

#[test]
fn recursive_provider_values_keep_the_complete_contract_beyond_sampling_depth() -> Result<()> {
    let schema = json!({"type":"object","required":["value"],"additionalProperties":false,"properties":{"value":{"$ref":"#/$defs/Node","x-fixture":{"provider":"tree"}}},"$defs":{"Node":{"type":"object","required":["child"],"properties":{"child":{"anyOf":[{"type":"null"},{"$ref":"#/$defs/Node"}]}},"additionalProperties":false}}});
    let factory = Factory::with_providers(&schema, BTreeSet::from(["tree".to_owned()]))?;
    let tree = json!({"child":{"child":{"child":null}}});
    let expected = json!({"value":tree});
    assert!(factory.is_valid(&expected));
    let values = factory.generate_with(
        &GenerationOptions {
            count: 10,
            seed: Some(42),
            profile: Profile::Realistic,
            ..GenerationOptions::default()
        },
        &mut |context| {
            let checker = jsonschema::options()
                .build(&context.schema)
                .map_err(|error| Error::Provider(error.to_string()))?;
            if !checker.is_valid(&tree) {
                return Err(Error::Provider(
                    "Context schema rejected the declared tree.".to_owned(),
                ));
            }
            Ok(tree.clone())
        },
    )?;
    assert_eq!(values, vec![expected; 10]);
    Ok(())
}

#[test]
fn referenced_positive_examples_are_checked_before_generation() {
    let schema = json!({"type":"object","properties":{"value":{"$ref":"#/$defs/Count","x-fixture":{"examples":[-1]}}},"$defs":{"Count":{"type":"integer","minimum":0}}});
    assert!(
        matches!(Factory::new(&schema), Err(Error::Schema(message)) if message.contains("Positive shorthand examples"))
    );
}

#[test]
fn openapi_references_inherit_document_dialects_and_discover_schema_resources() -> Result<()> {
    for reference in ["#/components/schemas/Body", "https://example.test/body"] {
        let document = json!({"openapi":"3.1.0","jsonSchemaDialect":"http://json-schema.org/draft-07/schema#","paths":{"/items":{"post":{"requestBody":{"content":{"application/json":{"schema":{"$ref":reference}}}}}}},"components":{"schemas":{"Body":{"$id":"https://example.test/body","type":"array","items":[{"enum":[7]}],"additionalItems":false,"minItems":1}}}});
        let session = Session::new(
            json!({"kind":"inline","value":document}).to_string(),
            r#"{"operation":{"path":"/items","method":"post","body":"request"}}"#,
            NumericMode::Exact,
        )?;
        let values: Value =
            serde_json::from_str(&session.generate(r#"{"count":10,"seed":42}"#, &mut |_| {
                Err(Error::Provider("No custom provider is needed.".to_owned()))
            })?)?;
        assert_eq!(values, json!(vec![vec![7]; 10]));
    }
    Ok(())
}

#[test]
fn property_names_preserve_percent_sequences_and_unicode() -> Result<()> {
    for name in ["a%2Fb", "a%20b%2Fc", "é/日~本"] {
        let schema = json!({"type":"object","required":[name],"properties":{name:{"const":7}},"additionalProperties":false});
        assert_eq!(
            Factory::new(&schema)?.generate(&GenerationOptions::default())?,
            vec![json!({name:7})]
        );
    }
    Ok(())
}

#[test]
fn openapi_indexing_preserves_real_resources_and_rejects_invented_pointer_targets() -> Result<()> {
    let operation = r#"{"operation":{"path":"/items","method":"post","body":"request"}}"#;
    let component = json!({"$id":"https://example.test/body","type":"integer","const":7});
    let mut document = json!({"openapi":"3.1.0","paths":{"/items":{"post":{"requestBody":{"content":{"application/json":{"schema":{"$ref":"https://example.test/body"}}}}}}},"components":{"schemas":{"selected_body":component}}});
    let session = Session::new(
        json!({"kind":"inline","value":document}).to_string(),
        operation,
        NumericMode::Exact,
    )?;
    assert_eq!(
        session.generate("{}", &mut |_| Err(Error::Provider("No callback needed.".to_owned())))?,
        "[7]"
    );
    document = json!({"openapi":"3.1.0","paths":{"/items":{"post":{"requestBody":{"content":{"application/json":{"schema":{"$ref":"https://example.test/inline"}}}}}},"/other":{"get":{"responses":{"200":{"content":{"application/json":{"schema":{"$id":"https://example.test/inline","type":"integer","const":7}}}}}}}}});
    let session = Session::new(
        json!({"kind":"inline","value":document}).to_string(),
        operation,
        NumericMode::Exact,
    )?;
    assert_eq!(
        session.generate("{}", &mut |_| Err(Error::Provider("No callback needed.".to_owned())))?,
        "[7]"
    );
    for reference in ["#/$defs/Body", "#/$defs/schema_0", "#/allOf/0"] {
        let document = json!({"openapi":"3.1.0","paths":{"/items":{"post":{"requestBody":{"content":{"application/json":{"schema":{"$ref":reference}}}}}}},"components":{"schemas":{"Body":{"const":7}}}});
        assert!(matches!(
            Session::new(
                json!({"kind":"inline","value":document}).to_string(),
                operation,
                NumericMode::Exact
            ),
            Err(Error::Schema(_))
        ));
    }
    Ok(())
}

#[test]
fn recursive_and_dynamic_anchors_apply_outer_constraints_to_nested_values() -> Result<()> {
    for recursive in [false, true] {
        for dynamic in [false, true] {
            let anchor = if recursive { "$recursiveAnchor" } else { "$dynamicAnchor" };
            let anchor_value = if recursive { json!(true) } else { json!("node") };
            let dialect = if recursive {
                "https://json-schema.org/draft/2019-09/schema"
            } else {
                "https://json-schema.org/draft/2020-12/schema"
            };
            let item = match (recursive, dynamic) {
                (true, true) => json!({"$recursiveRef":"#"}),
                (true, false) => json!({"$ref":"https://example.test/tree"}),
                (false, true) => json!({"$dynamicRef":"#node"}),
                (false, false) => json!({"$ref":"https://example.test/tree#node"}),
            };
            let tree = json!({"$id":"https://example.test/tree",anchor:anchor_value,"type":"object","properties":{"data":true,"children":{"type":"array","items":item}}});
            let schema = json!({"$id":"https://example.test/strict","$schema":dialect,anchor:anchor_value,"$ref":"https://example.test/tree","unevaluatedProperties":false,"$defs":{"Tree":tree}});
            let factory = Factory::new(&schema)?;
            assert!(factory.is_valid(&json!({"children":[{"data":1}]})));
            assert_eq!(factory.is_valid(&json!({"children":[{"unexpected":1}]})), !dynamic);
        }
    }
    let factory = Factory::new(&json!({"$ref":"#","type":"integer","const":17}))?;
    assert_eq!(
        factory.generate(&GenerationOptions {
            count: 4,
            seed: Some(42),
            ..GenerationOptions::default()
        })?,
        vec![json!(17); 4]
    );
    Ok(())
}

#[test]
fn nested_openapi_resources_keep_their_own_dialects_across_references() -> Result<()> {
    for (dialect, body) in [
        (
            "https://json-schema.org/draft/2020-12/schema",
            json!({"$id":"https://example.test/body","$schema":"http://json-schema.org/draft-07/schema#","$ref":"#/definitions/Tuple","definitions":{"Tuple":{"type":"array","items":[{"const":7}],"additionalItems":false,"minItems":1}}}),
        ),
        (
            "http://json-schema.org/draft-07/schema#",
            json!({"$id":"https://example.test/body","$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/Tuple","$defs":{"Tuple":{"type":"array","prefixItems":[{"const":7}],"items":false,"minItems":1}}}),
        ),
    ] {
        let document = json!({"openapi":"3.1.0","jsonSchemaDialect":dialect,"paths":{"/items":{"post":{"requestBody":{"content":{"application/json":{"schema":{"$ref":"https://example.test/body"}}}}}}},"components":{"schemas":{"Body":body}}});
        let session = Session::new(
            json!({"kind":"inline","value":document}).to_string(),
            r#"{"operation":{"path":"/items","method":"post","body":"request"}}"#,
            NumericMode::Exact,
        )?;
        assert_eq!(
            session
                .generate("{}", &mut |_| Err(Error::Provider("No callback needed.".to_owned())))?,
            "[[7]]"
        );
    }
    Ok(())
}

#[test]
fn draft4_integer_tokens_remain_distinct_from_integer_valued_decimals() -> Result<()> {
    let decimal: Value = serde_json::from_str("1.0")?;
    for (schema, invalid, valid) in [
        (
            json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"integer"}),
            decimal.clone(),
            json!(1),
        ),
        (
            json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"array","items":{"type":"integer"}}),
            json!([decimal]),
            json!([1]),
        ),
        (
            json!({"$schema":"http://json-schema.org/draft-04/schema#","type":["integer","string"]}),
            serde_json::from_str("1.0")?,
            json!("one"),
        ),
    ] {
        let factory = Factory::new(&schema)?;
        assert!(factory.is_valid(&valid));
        assert!(!factory.is_valid(&invalid));
    }
    let schema = json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"integer","x-fixture":{"examples":[serde_json::from_str::<Value>("1.0")?]}});
    assert!(matches!(Factory::new(&schema), Err(Error::Schema(_))));
    Ok(())
}
