//! Complete public workflows for common providers, input contracts, and invocation rules.
use std::collections::BTreeSet;

use riff_core::{
    Error, Factory, GenerationOptions, NumericMode, Profile, Result, Session, parse_document,
};
use serde_json::{Value, json};

#[test]
fn defaults_generate_consumer_ready_localized_records_without_host_generators() -> Result<()> {
    let schema = json!({
        "type":"object", "additionalProperties":false,
        "required":["name","email","company","city","phone","country","url","id"],
        "properties": {
            "name":{"type":"string","minLength":1,"x-fixture":{"provider":"full-name","locale":"ja-JP"}},
            "email":{"type":"string","format":"email","x-fixture":{"provider":"email"}},
            "company":{"type":"string","minLength":1,"x-fixture":{"provider":"company-name"}},
            "city":{"type":"string","minLength":1,"x-fixture":{"provider":"city","locale":"fr-FR"}},
            "phone":{"type":"string","minLength":1,"x-fixture":{"provider":"phone-number"}},
            "country":{"type":"string","pattern":"^[A-Z]{2}$","x-fixture":{"provider":"country-code"}},
            "url":{"type":"string","format":"uri","x-fixture":{"provider":"url"}},
            "id":{"type":"string","format":"uuid","x-fixture":{"provider":"uuid"}}
        }
    });
    let factory = Factory::with_providers(&schema, BTreeSet::new())?;
    let options = GenerationOptions {
        count: 20,
        seed: Some(42),
        profile: Profile::Realistic,
        ..GenerationOptions::default()
    };
    let mut no_host = |_: &riff_core::ProviderContext| {
        Err(Error::Provider("Unexpected host invocation.".to_owned()))
    };
    let first = factory.generate_with(&options, &mut no_host)?;
    assert_eq!(first, factory.generate_with(&options, &mut no_host)?);
    for record in first {
        assert!(record.get("name").and_then(Value::as_str).is_some_and(|name| {
            name.chars().any(|character| ('\u{3040}'..='\u{9fff}').contains(&character))
        }));
        assert!(
            record.get("email").and_then(Value::as_str).is_some_and(|email| email.contains('@'))
        );
        assert!(
            record
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|url| url.starts_with("https://") && url.ends_with(".example.test/"))
        );
        assert!(
            record.get("id").and_then(Value::as_str).is_some_and(|id| id.len() == 36
                && id.split('-').nth(2).is_some_and(|part| part.starts_with('4')))
        );
        assert!(record.get("country").and_then(Value::as_str).is_some_and(
            |code| code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_uppercase())
        ));
    }
    Ok(())
}

#[test]
fn contract_parser_preserves_numeric_lexemes_and_rejects_ambiguous_yaml() -> Result<()> {
    let schema =
        parse_document("type: integer\nminimum: 9007199254740993\nmaximum: 9007199254740993\n")?;
    let factory = Factory::with_providers(&schema, BTreeSet::new())?;
    let options = GenerationOptions::default();
    let values = factory.generate_with(&options, &mut |_| {
        Err(Error::Provider("Unexpected host invocation.".to_owned()))
    })?;
    assert_eq!(values, vec![json!(9_007_199_254_740_993_i64)]);
    for source in [
        "type: string\ntype: integer\n",
        "a: &a [1]\nb: *a\n",
        "!!python/object/apply:os.system ['ignored']",
        "type: string\n---\ntype: integer",
        "minimum: .nan",
        "{1: value}",
    ] {
        assert!(
            matches!(parse_document(source), Err(Error::Schema(_))),
            "Accepted invalid JSON contract: {source}"
        );
    }
    Ok(())
}

#[test]
fn signed_and_large_seeds_replay_identical_public_batches() -> Result<()> {
    let session = Session::new(
        json!({"kind":"inline","value":{"type":"integer","minimum":1,"maximum":50}}).to_string(),
        "{}",
        NumericMode::Exact,
    )?;
    let mut no_host = |_: &riff_core::ProviderContext| {
        Err(Error::Provider("Unexpected host invocation.".to_owned()))
    };
    let negative = session.generate(r#"{"count":20,"seed":"-1"}"#, &mut no_host)?;
    assert_eq!(
        negative,
        session.generate(r#"{"count":20,"seed":"18446744073709551615"}"#, &mut no_host)?
    );
    assert_eq!(
        negative,
        session.generate(r#"{"count":20,"seed":"36893488147419103231"}"#, &mut no_host)?
    );
    let values: Vec<u8> = serde_json::from_str(&negative)?;
    assert_eq!(values.len(), 20);
    assert!(values.into_iter().all(|value| (1..=50).contains(&value)));
    for options in [
        r#"{"count":true}"#,
        r#"{"count":-1}"#,
        r#"{"profile":"invalid"}"#,
        r#"{"maxAttempts":0}"#,
        r#"{"seed":false}"#,
        "[]",
        r#"[2,null,"mixed",100]"#,
    ] {
        assert!(matches!(session.generate(options, &mut no_host), Err(Error::Argument(_))));
    }
    Ok(())
}
