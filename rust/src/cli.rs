//! One command-line workflow for every executable interface.
use clap::Parser;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{Body, Error, NumericMode, ProviderContext, Result, Session};

#[derive(Debug, Parser)]
#[command(name = "riff", about = "Generate schema-guided API fuzz fixtures.")]
struct Arguments {
    source: std::path::PathBuf,
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    method: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    media_type: Option<String>,
    #[arg(long, value_enum)]
    body: Option<Body>,
    #[arg(long)]
    fixtures: Option<std::path::PathBuf>,
    #[arg(long)]
    count: Option<usize>,
    #[arg(long, allow_hyphen_values = true)]
    seed: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    max_attempts: Option<usize>,
    #[arg(long)]
    expectation: Option<String>,
    #[arg(long)]
    jsonl: bool,
}

/// Complete CLI output, ready for the executable to write to its standard streams.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliOutput {
    /// Complete stdout, never a partial generated batch.
    pub stdout: String,
    /// Diagnostic text or an empty string on success.
    pub stderr: String,
    /// Zero on success/help, two on invalid input or generation failure.
    pub exit_code: i32,
}

/// Execute the command workflow without writing process-global standard streams.
#[must_use]
pub fn run_cli(arguments: Vec<String>, numbers: NumericMode) -> CliOutput {
    let arguments =
        match Arguments::try_parse_from(std::iter::once("riff".to_owned()).chain(arguments)) {
            Ok(arguments) => arguments,
            Err(error) => {
                return CliOutput {
                    stdout: if error.use_stderr() { String::new() } else { error.to_string() },
                    stderr: if error.use_stderr() { error.to_string() } else { String::new() },
                    exit_code: error.exit_code(),
                };
            }
        };
    match execute(arguments, numbers) {
        Ok(stdout) => CliOutput { stdout, stderr: String::new(), exit_code: 0 },
        Err(error) => {
            CliOutput { stdout: String::new(), stderr: format!("riff: {error}\n"), exit_code: 2 }
        }
    }
}

fn execute(arguments: Arguments, numbers: NumericMode) -> Result<String> {
    let operation = arguments.path.map(|path| {
        let mut operation = serde_json::Map::new();
        let _ = operation.insert("path".to_owned(), Value::String(path));
        for (key, value) in [
            ("method", arguments.method.map(Value::String)),
            ("status", arguments.status.map(Value::String)),
            ("body", arguments.body.map(|body| json!(body))),
            ("mediaType", arguments.media_type.map(Value::String)),
        ] {
            if let Some(value) = value {
                let _ = operation.insert(key.to_owned(), value);
            }
        }
        Value::Object(operation)
    });
    let session = Session::new(
        json!({"kind":"file", "value":arguments.source}).to_string(),
        json!({"operation":operation,"fixtures":arguments.fixtures.map(|file| json!({"kind":"file","value":file}))}).to_string(), numbers,
    )?;
    let mut options: serde_json::Map<String, Value> = [
        ("count", arguments.count.map(Value::from)),
        ("seed", arguments.seed.map(Value::String)),
        ("profile", arguments.profile.map(Value::String)),
        ("maxAttempts", arguments.max_attempts.map(Value::from)),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
    .collect();
    let mut no_custom_provider = |_: &ProviderContext| {
        Err(Error::Provider("The CLI cannot invoke custom providers.".to_owned()))
    };
    let output = if let Some(expectation) = arguments.expectation {
        let _ = options.insert("expectation".to_owned(), expectation.into());
        let source = session.cases(Value::Object(options).to_string(), &mut no_custom_provider)?;
        let mut cases: Value = serde_json::from_str(&source)?;
        if let Some(cases) = cases.as_array_mut() {
            for case in cases {
                if let Some(case) = case.as_object_mut() {
                    let _ = case.remove("payload");
                }
            }
        }
        serde_json::to_string(&cases)?
    } else {
        session.generate(Value::Object(options).to_string(), &mut no_custom_provider)?
    };
    let value: Value = serde_json::from_str(&output)?;
    if arguments.jsonl {
        let Value::Array(values) = value else {
            return Err(Error::Generation("Expected fixture batch.".to_owned()));
        };
        return Ok(values
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n")
            + if values.is_empty() { "" } else { "\n" });
    }
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}
