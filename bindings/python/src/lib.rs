//! Python values, callbacks, and errors translated for fixture operations.
mod factory;
mod json;
mod metadata;
mod results;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use riff_core::NumericMode;

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

#[pyfunction]
fn strip_fixture_metadata(py: Python<'_>, document: json::JsonInput) -> PyResult<Py<PyAny>> {
    let json::JsonInput(document) = document;
    json::to_python(py, &riff_core::strip_metadata(&document))
}

#[pyfunction]
fn encode_json(value: json::JsonInput) -> String {
    let json::JsonInput(value) = value;
    value.to_string()
}

#[pyfunction]
fn cli(arguments: Vec<String>) -> (String, String, i32) {
    let output = riff_core::run_cli(arguments, NumericMode::NativeFloat);
    (output.stdout, output.stderr, output.exit_code)
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<factory::FixtureFactory>()?;
    module.add_class::<results::SuccessCase>()?;
    module.add_class::<results::FailureCase>()?;
    module.add_class::<results::Violation>()?;
    module.add_class::<results::MinimizationResult>()?;
    module.add_class::<results::ProviderContext>()?;
    metadata::register(module)?;
    let python = module.py();
    module.add("RiffError", python.get_type::<RiffError>())?;
    module.add("SchemaError", python.get_type::<SchemaError>())?;
    module.add("GenerationError", python.get_type::<GenerationError>())?;
    module.add("ProviderError", python.get_type::<ProviderError>())?;
    module.add_function(wrap_pyfunction!(strip_fixture_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(encode_json, module)?)?;
    module.add_function(wrap_pyfunction!(factory::model_is_valid, module)?)?;
    module.add_function(wrap_pyfunction!(cli, module)?)?;
    Ok(())
}
