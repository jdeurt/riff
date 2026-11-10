//! Generate Riff's inputs directly from their complete schemas.
use std::collections::BTreeSet;

use riff_core::{Error, Factory, GenerationOptions, Result};
use serde_json::Value;

#[test]
fn generate_large_batches_from_full_input_contracts() -> Result<()> {
    for source in [
        include_str!("../schemas/json-schema.json"),
        include_str!("../schemas/openapi.json"),
        include_str!("../schemas/sidecar.json"),
    ] {
        let schema: Value = serde_json::from_str(source)?;
        let factory = Factory::new(&schema)?;
        let validator = jsonschema::options()
            .build(&schema)
            .map_err(|error| Error::Schema(error.to_string()))?;
        let options =
            GenerationOptions { count: 1024, seed: Some(42), ..GenerationOptions::default() };
        let values = factory.generate(&options).map_err(|error| {
            Error::Generation(format!("{}: {error}", schema.get("title").unwrap_or(&Value::Null)))
        })?;
        assert_eq!(values.len(), 1024);
        for value in &values {
            assert!(validator.is_valid(value), "Invalid generated input: {value}");
        }
        let distinct: BTreeSet<_> = values.iter().map(Value::to_string).collect();
        assert!(distinct.len() >= 32, "Expected varied input documents, got {}", distinct.len());
    }
    Ok(())
}
