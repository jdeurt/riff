//! Python values, callbacks, and errors translated for fixture operations.
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyFunction;
use riff_core::{NumericMode, ProviderContext, Session};
use serde_json::Value;

create_exception!(
    _native,
    RiffError,
    PyException,
    "A Riff contract, provider, or generation failure."
);
create_exception!(_native, SchemaError, RiffError, "Invalid or unsupported contract.");
create_exception!(_native, GenerationError, RiffError, "Bounded generation exhausted.");
create_exception!(
    _native,
    ProviderError,
    RiffError,
    "A provider failed or returned non-JSON data."
);

fn python_error(error: riff_core::Error) -> PyErr {
    match error {
        riff_core::Error::Argument(message) => PyValueError::new_err(message),
        riff_core::Error::Schema(message) => SchemaError::new_err(message),
        riff_core::Error::Generation(message) => GenerationError::new_err(message),
        riff_core::Error::Provider(message) => ProviderError::new_err(message),
    }
}

fn provider_call(
    function: &Bound<'_, PyFunction>,
    context: &ProviderContext,
    failure: &mut Option<PyErr>,
) -> riff_core::Result<Value> {
    let request = serde_json::to_string(context)?;
    match function.call1((request,)).and_then(|value| value.extract::<String>()) {
        Ok(output) => serde_json::from_str(&output)
            .map_err(|error| riff_core::Error::Provider(error.to_string())),
        Err(error) => {
            let message = error.to_string();
            *failure = Some(error);
            Err(riff_core::Error::Provider(message))
        }
    }
}

#[pyclass(frozen, module = "riff._native")]
struct NativeFactory {
    session: Session,
}

#[pymethods]
impl NativeFactory {
    #[new]
    fn new(source: &str, configuration: &str) -> PyResult<Self> {
        let session =
            Session::new(source, configuration, NumericMode::NativeFloat).map_err(python_error)?;
        Ok(Self { session })
    }

    fn generate(&self, options: &str, provider: &Bound<'_, PyFunction>) -> PyResult<String> {
        let mut failure = None;
        let result = self
            .session
            .generate(options, &mut |context| provider_call(provider, context, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        result.map_err(python_error)
    }

    fn cases(&self, options: &str, provider: &Bound<'_, PyFunction>) -> PyResult<String> {
        let mut failure = None;
        let result = self
            .session
            .cases(options, &mut |context| provider_call(provider, context, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        result.map_err(python_error)
    }

    fn minimize(
        &self,
        value: &str,
        options: &str,
        oracle: &Bound<'_, PyFunction>,
    ) -> PyResult<String> {
        let mut failure = None;
        let result = self.session.minimize(value, options, &mut |value| {
            let payload = serde_json::to_string(value)?;
            match oracle.call1((payload,)).and_then(|value| value.extract::<bool>()) {
                Ok(result) => Ok(result),
                Err(error) => {
                    let message = error.to_string();
                    failure = Some(error);
                    Err(riff_core::Error::Provider(message))
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        result.map_err(python_error)
    }

    fn is_valid(&self, value: &str) -> PyResult<bool> {
        self.session.is_valid(value).map_err(python_error)
    }
}

#[pyfunction]
fn strip_metadata(document: &str) -> PyResult<String> {
    let value: Value =
        serde_json::from_str(document).map_err(riff_core::Error::from).map_err(python_error)?;
    serde_json::to_string(&riff_core::strip_metadata(&value))
        .map_err(riff_core::Error::from)
        .map_err(python_error)
}

#[pyfunction]
fn validate_metadata(metadata: &str) -> PyResult<()> {
    let value: Value =
        serde_json::from_str(metadata).map_err(riff_core::Error::from).map_err(python_error)?;
    riff_core::Metadata::parse(Some(&value))
        .map(|_| ())
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pyfunction]
fn cli(arguments: Vec<String>) -> (String, String, i32) {
    let output = riff_core::run_cli(arguments, NumericMode::NativeFloat);
    (output.stdout, output.stderr, output.exit_code)
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeFactory>()?;
    let python = module.py();
    module.add("RiffError", python.get_type::<RiffError>())?;
    module.add("SchemaError", python.get_type::<SchemaError>())?;
    module.add("GenerationError", python.get_type::<GenerationError>())?;
    module.add("ProviderError", python.get_type::<ProviderError>())?;
    module.add_function(wrap_pyfunction!(strip_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(validate_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(cli, module)?)?;
    Ok(())
}
