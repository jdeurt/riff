//! `PyO3` binding for the shared Riff engine.
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyFunction;
use riff_core::{Factory, GenerationOptions, ProviderContext};
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
    core: Factory,
}

#[pymethods]
impl NativeFactory {
    #[new]
    #[pyo3(signature = (document, names, locales, operation=None, fixtures=None))]
    fn new(
        document: &str,
        names: Vec<String>,
        locales: Vec<String>,
        operation: Option<&str>,
        fixtures: Option<&str>,
    ) -> PyResult<Self> {
        let document: Value =
            serde_json::from_str(document).map_err(riff_core::Error::from).map_err(python_error)?;
        let operation = operation
            .map(serde_json::from_str)
            .transpose()
            .map_err(riff_core::Error::from)
            .map_err(python_error)?;
        let fixtures = fixtures
            .map(serde_json::from_str)
            .transpose()
            .map_err(riff_core::Error::from)
            .map_err(python_error)?;
        let schema = riff_core::prepare(&document, operation.as_ref(), fixtures.as_ref())
            .map_err(python_error)?;
        let core =
            Factory::new(&schema, names.into_iter().collect(), locales.into_iter().collect())
                .map_err(python_error)?;
        Ok(Self { core })
    }

    fn generate(&self, options: &str, provider: &Bound<'_, PyFunction>) -> PyResult<String> {
        let options: GenerationOptions =
            serde_json::from_str(options).map_err(riff_core::Error::from).map_err(python_error)?;
        let mut failure = None;
        let result = self
            .core
            .generate(&options, &mut |context| provider_call(provider, context, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        serde_json::to_string(&result.map_err(python_error)?)
            .map_err(riff_core::Error::from)
            .map_err(python_error)
    }

    fn failure_cases(&self, options: &str, provider: &Bound<'_, PyFunction>) -> PyResult<String> {
        let options: GenerationOptions =
            serde_json::from_str(options).map_err(riff_core::Error::from).map_err(python_error)?;
        let mut failure = None;
        let result = self
            .core
            .failure_cases(&options, &mut |context| provider_call(provider, context, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        serde_json::to_string(&result.map_err(python_error)?)
            .map_err(riff_core::Error::from)
            .map_err(python_error)
    }

    fn minimize(
        &self,
        value: &str,
        maximum: usize,
        preserve: bool,
        oracle: &Bound<'_, PyFunction>,
    ) -> PyResult<String> {
        let input: Value =
            serde_json::from_str(value).map_err(riff_core::Error::from).map_err(python_error)?;
        let mut failure = None;
        let result = self.core.minimize(&input, maximum, preserve, &mut |value| {
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
        serde_json::to_string(&result.map_err(python_error)?)
            .map_err(riff_core::Error::from)
            .map_err(python_error)
    }

    fn is_valid(&self, value: &str) -> PyResult<bool> {
        let value: Value =
            serde_json::from_str(value).map_err(riff_core::Error::from).map_err(python_error)?;
        Ok(self.core.is_valid(&value))
    }

    fn schema(&self) -> PyResult<String> {
        serde_json::to_string(&self.core.schema())
            .map_err(riff_core::Error::from)
            .map_err(python_error)
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

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeFactory>()?;
    let python = module.py();
    module.add("RiffError", python.get_type::<RiffError>())?;
    module.add("SchemaError", python.get_type::<SchemaError>())?;
    module.add("GenerationError", python.get_type::<GenerationError>())?;
    module.add("ProviderError", python.get_type::<ProviderError>())?;
    module.add_function(wrap_pyfunction!(strip_metadata, module)?)?;
    Ok(())
}
