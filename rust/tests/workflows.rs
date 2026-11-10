//! Public Rust workflows complement the language bindings' integration suites.
use std::collections::BTreeSet;

use riff_core::{Error, Factory, GenerationOptions, ProviderContext, Result};
use serde_json::{Value, json};

#[test]
fn semantic_providers_drive_replayable_valid_batches_and_declared_failures() -> Result<()> {
    let schema = json!({
        "type": "object", "required": ["id", "status"], "additionalProperties": false,
        "properties": {
            "id": { "type": "string", "pattern": "^acct_[0-9a-f]{16}$", "x-fixture": {"provider": "acme.id"} },
            "status": { "enum": ["active", "inactive"], "x-fixture": {"strategy": {"kind": "values", "values": ["active"]}, "fail": {"examples": ["inactive"]}} }
        }
    });
    let factory = Factory::with_providers(&schema, BTreeSet::from(["acme.id".to_owned()]))?;
    let options = GenerationOptions {
        count: 20,
        seed: Some(u64::MAX),
        profile: riff_core::Profile::Realistic,
        ..GenerationOptions::default()
    };
    let mut provider = |_context: &ProviderContext| Ok(json!("acct_0123456789abcdef"));
    let first = factory.generate_with(&options, &mut provider)?;
    assert_eq!(first, factory.generate_with(&options, &mut provider)?);
    assert_eq!(first.len(), 20);
    assert!(first.iter().all(|value| value.get("id") == Some(&json!("acct_0123456789abcdef"))
        && value.get("status") == Some(&json!("active"))));
    for case in factory.failure_cases_with(&options, &mut provider)? {
        assert_eq!(case.target.as_deref(), Some("/status"));
        assert_eq!(case.value.get("status"), Some(&json!("inactive")));
        assert!(case.schema_valid);
        assert!(case.violations.is_empty());
    }
    Ok(())
}

#[test]
fn minimization_keeps_the_caller_failure_and_original_input() -> Result<()> {
    let factory = Factory::with_providers(
        &json!({"type":"object", "required":["name"], "properties":{"name":{"type":"string","minLength":1}}}),
        BTreeSet::new(),
    )?;
    let original = json!({"name":"prefix!suffix", "unrelated":[1,2,3]});
    let result = factory.minimize(&original, 100, true, &mut |value| {
        Ok(value.get("name").and_then(Value::as_str).is_some_and(|name| name.contains('!')))
    })?;
    assert_eq!(result.value, json!({"name":"!"}));
    assert_eq!(original, json!({"name":"prefix!suffix", "unrelated":[1,2,3]}));
    assert!(result.evaluations <= 100);
    assert!(!result.budget_exhausted);
    Ok(())
}

#[test]
fn extreme_numeric_exponents_fail_before_decimal_expansion() -> Result<()> {
    for text in [
        r#"{"type":"number","minimum":1e1000000000,"maximum":1e1000000000}"#,
        r#"{"type":"integer","multipleOf":1e-1000000000}"#,
    ] {
        let schema: Value = serde_json::from_str(text)?;
        let result = Factory::with_providers(&schema, BTreeSet::new());
        assert!(
            matches!(result, Err(Error::Schema(message)) if message.contains("supported limit"))
        );
    }
    Ok(())
}

#[test]
fn exclusive_singleton_ranges_use_exact_rational_indexing() -> Result<()> {
    let lower = format!("1{}1", "0".repeat(99));
    let upper = format!("1{}2", "0".repeat(99));
    let schema: Value = serde_json::from_str(&format!(
        r#"{{"type":"integer","exclusiveMinimum":{lower},"maximum":{upper},"multipleOf":3}}"#
    ))?;
    let factory = Factory::with_providers(&schema, BTreeSet::new())?;
    let options = GenerationOptions { count: 3, seed: Some(42), ..GenerationOptions::default() };
    let expected: Value = serde_json::from_str(&upper)?;
    let actual = factory.generate_with(&options, &mut |_| {
        Err(Error::Provider("No provider is needed.".to_owned()))
    })?;
    assert_eq!(actual, vec![expected; 3]);
    Ok(())
}

#[test]
fn provider_numbers_obey_allocation_limits_before_validation() -> Result<()> {
    let factory = Factory::with_providers(
        &json!({"x-fixture":{"provider":"extreme"}}),
        BTreeSet::from(["extreme".to_owned()]),
    )?;
    let options = GenerationOptions {
        profile: riff_core::Profile::Realistic,
        ..GenerationOptions::default()
    };
    let result = factory.generate_with(&options, &mut |_| {
        serde_json::from_str("1e1000000000").map_err(Error::from)
    });
    assert!(matches!(result, Err(Error::Provider(message)) if message.contains("numeric")));
    Ok(())
}
