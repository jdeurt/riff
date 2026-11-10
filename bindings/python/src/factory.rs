//! Public factory methods translated directly to shared operations.
use std::collections::BTreeMap;
use std::path::PathBuf;

use pyo3::PyTraverseError;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::gc::PyVisit;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyType};
use riff_core::{NumericMode, Session};
use serde_json::{Map, Value, json};

use crate::json::{Integer, JsonInput, to_python};
use crate::results::{FailureCase, MinimizationResult, ProviderContext, SuccessCase, case_object};
use crate::{ProviderError, python_error};

/// Compile a contract once and generate independent, seeded batches.
#[pyclass(frozen, module = "riff._native")]
pub struct FixtureFactory {
    session: Session,
    providers: BTreeMap<String, Py<PyAny>>,
}

fn registry(mapping: Option<&Bound<'_, PyAny>>) -> PyResult<BTreeMap<String, Py<PyAny>>> {
    let mut providers = BTreeMap::new();
    if let Some(mapping) = mapping {
        for item in mapping.call_method0("items")?.try_iter()? {
            let (name, callback): (String, Bound<'_, PyAny>) = item?.extract()?;
            if !callback.is_callable() {
                return Err(PyValueError::new_err("Providers must be callable."));
            }
            let _ = providers.insert(name, callback.unbind());
        }
    }
    Ok(providers)
}

fn operation(path: &str, method: &str, status: &str, media_type: &str, body: &str) -> Value {
    json!({"path":path,"method":method,"status":status,"mediaType":media_type,"body":body})
}

fn controls(
    count: Integer,
    seed: Option<Integer>,
    profile: &str,
    max_attempts: Integer,
    expectation: Option<&str>,
) -> String {
    let mut value = Map::from_iter([
        ("count".to_owned(), count.0),
        ("seed".to_owned(), seed.map_or(Value::Null, |value| value.0)),
        ("profile".to_owned(), Value::String(profile.to_owned())),
        ("maxAttempts".to_owned(), max_attempts.0),
    ]);
    if let Some(expectation) = expectation {
        let _ = value.insert("expectation".to_owned(), Value::String(expectation.to_owned()));
    }
    Value::Object(value).to_string()
}

impl FixtureFactory {
    fn compile(
        source: &Value,
        providers: Option<&Bound<'_, PyAny>>,
        fixture_file: Option<PathBuf>,
        operation: Option<&Value>,
    ) -> PyResult<Self> {
        let providers = registry(providers)?;
        let fixtures = fixture_file
            .map(|path| {
                let path = path.into_os_string().into_string().map_err(|_| {
                    PyValueError::new_err("Fixture file paths must be valid UTF-8.")
                })?;
                Ok::<_, PyErr>(json!({"kind":"file","value":path}))
            })
            .transpose()?;
        let configuration = json!({"providers": providers.keys().collect::<Vec<_>>(),"fixtures":fixtures,"operation":operation});
        let session =
            Session::new(source.to_string(), configuration.to_string(), NumericMode::NativeFloat)
                .map_err(python_error)?;
        Ok(Self { session, providers })
    }

    fn provider(
        &self,
        py: Python<'_>,
        context: &riff_core::ProviderContext,
        failure: &mut Option<PyErr>,
    ) -> riff_core::Result<Value> {
        let result = self
            .providers
            .get(&context.name)
            .ok_or_else(|| PyValueError::new_err(format!("Unknown provider: {}", context.name)))
            .and_then(|callback| {
                let argument = Py::new(py, ProviderContext::from_context(py, context)?)?;
                callback.bind(py).call1((argument,))?.extract::<JsonInput>().map(|value| value.0)
            });
        result.map_err(|cause| {
            let message = format!("Provider {:?} failed: {cause}", context.name);
            let error = if cause.is_instance_of::<PyException>(py) {
                let error = ProviderError::new_err(message.clone());
                error.set_cause(py, Some(cause));
                error
            } else {
                cause
            };
            *failure = Some(error);
            riff_core::Error::Provider(message)
        })
    }

    fn batch_cases(&self, py: Python<'_>, options: &str) -> PyResult<Vec<riff_core::Case>> {
        let mut failure = None;
        let result =
            self.session.cases(options, &mut |context| self.provider(py, context, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        serde_json::from_str(&result.map_err(python_error)?)
            .map_err(|error| python_error(error.into()))
    }
}

#[pymethods]
impl FixtureFactory {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        for callback in self.providers.values() {
            visit.call(callback)?;
        }
        Ok(())
    }

    #[new]
    #[pyo3(signature = (schema, *, providers=None, fixture_file=None))]
    fn new(
        schema: JsonInput,
        providers: Option<&Bound<'_, PyAny>>,
        fixture_file: Option<PathBuf>,
    ) -> PyResult<Self> {
        let JsonInput(schema) = schema;
        Self::compile(&json!({"kind":"inline","value":schema}), providers, fixture_file, None)
    }

    #[classmethod]
    #[pyo3(signature = (schema, *, providers=None, fixture_file=None))]
    fn from_schema(
        _cls: &Bound<'_, PyType>,
        schema: JsonInput,
        providers: Option<&Bound<'_, PyAny>>,
        fixture_file: Option<PathBuf>,
    ) -> PyResult<Self> {
        Self::new(schema, providers, fixture_file)
    }

    #[classmethod]
    #[pyo3(signature = (document, *, path, method="get", status="200", media_type="application/json", body="response", providers=None, fixture_file=None))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Translate the public keyword-only operation selector."
    )]
    fn from_openapi(
        _cls: &Bound<'_, PyType>,
        document: JsonInput,
        path: &str,
        method: &str,
        status: &str,
        media_type: &str,
        body: &str,
        providers: Option<&Bound<'_, PyAny>>,
        fixture_file: Option<PathBuf>,
    ) -> PyResult<Self> {
        let JsonInput(document) = document;
        Self::compile(
            &json!({"kind":"inline","value":document}),
            providers,
            fixture_file,
            Some(&operation(path, method, status, media_type, body)),
        )
    }

    #[classmethod]
    #[pyo3(signature = (source, *, path=None, method="get", status="200", media_type="application/json", body="response", providers=None, fixture_file=None))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Translate the public file and operation selectors."
    )]
    fn from_file(
        _cls: &Bound<'_, PyType>,
        source: PathBuf,
        path: Option<&str>,
        method: &str,
        status: &str,
        media_type: &str,
        body: &str,
        providers: Option<&Bound<'_, PyAny>>,
        fixture_file: Option<PathBuf>,
    ) -> PyResult<Self> {
        let source = source
            .into_os_string()
            .into_string()
            .map_err(|_| PyValueError::new_err("Contract file paths must be valid UTF-8."))?;
        Self::compile(
            &json!({"kind":"file","value":source}),
            providers,
            fixture_file,
            path.map(|path| operation(path, method, status, media_type, body)).as_ref(),
        )
    }

    #[classmethod]
    #[pyo3(signature = (model, *, providers=None))]
    fn for_model(
        cls: &Bound<'_, PyType>,
        model: &Bound<'_, PyAny>,
        providers: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let schema = model.call_method0("model_json_schema")?.extract::<JsonInput>()?;
        let factory = Py::new(cls.py(), Self::new(schema, providers, None)?)?;
        cls.py()
            .import("riff.models")?
            .getattr("ModelFactory")?
            .call1((factory, model))
            .map(Bound::unbind)
    }

    fn as_model(slf: PyRef<'_, Self>, model: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        model
            .py()
            .import("riff.models")?
            .getattr("ModelFactory")?
            .call1((slf, model))
            .map(Bound::unbind)
    }

    #[pyo3(signature = (*, count=Integer::small(1), seed=None, profile="mixed", max_attempts=Integer::small(100)), text_signature="($self, *, count=1, seed=None, profile='mixed', max_attempts=100)")]
    fn generate(
        &self,
        py: Python<'_>,
        count: Integer,
        seed: Option<Integer>,
        profile: &str,
        max_attempts: Integer,
    ) -> PyResult<Py<PyAny>> {
        let options = controls(count, seed, profile, max_attempts, None);
        let mut failure = None;
        let result =
            self.session.generate(options, &mut |context| self.provider(py, context, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        let value = serde_json::from_str(&result.map_err(python_error)?)
            .map_err(|error| python_error(error.into()))?;
        to_python(py, &value)
    }

    #[pyo3(signature = (*, count=Integer::small(1), seed=None, profile="mixed", max_attempts=Integer::small(100)), text_signature="($self, *, count=1, seed=None, profile='mixed', max_attempts=100)")]
    fn success_cases(
        &self,
        py: Python<'_>,
        count: Integer,
        seed: Option<Integer>,
        profile: &str,
        max_attempts: Integer,
    ) -> PyResult<Vec<SuccessCase>> {
        self.batch_cases(py, &controls(count, seed, profile, max_attempts, Some("pass")))?
            .iter()
            .map(|case| SuccessCase::from_value(py, &case.value))
            .collect()
    }

    #[pyo3(signature = (*, count=Integer::small(1), seed=None, profile="mixed", max_attempts=Integer::small(100)), text_signature="($self, *, count=1, seed=None, profile='mixed', max_attempts=100)")]
    fn failure_cases(
        &self,
        py: Python<'_>,
        count: Integer,
        seed: Option<Integer>,
        profile: &str,
        max_attempts: Integer,
    ) -> PyResult<Vec<FailureCase>> {
        self.batch_cases(py, &controls(count, seed, profile, max_attempts, Some("fail")))?
            .into_iter()
            .map(|case| FailureCase::from_case(py, case))
            .collect()
    }

    #[pyo3(signature = (*, count=Integer::small(1), expectation="pass", seed=None, profile="mixed", max_attempts=Integer::small(100)), text_signature="($self, *, count=1, expectation='pass', seed=None, profile='mixed', max_attempts=100)")]
    fn generate_cases(
        &self,
        py: Python<'_>,
        count: Integer,
        expectation: &str,
        seed: Option<Integer>,
        profile: &str,
        max_attempts: Integer,
    ) -> PyResult<Vec<Py<PyAny>>> {
        self.batch_cases(py, &controls(count, seed, profile, max_attempts, Some(expectation)))?
            .into_iter()
            .map(|case| case_object(py, case))
            .collect()
    }

    #[pyo3(signature = (value, *, is_failure, max_evaluations=Integer::small(100), preserve_schema=true), text_signature="($self, value, *, is_failure, max_evaluations=100, preserve_schema=True)")]
    fn minimize(
        &self,
        py: Python<'_>,
        value: JsonInput,
        is_failure: &Bound<'_, PyAny>,
        max_evaluations: Integer,
        preserve_schema: bool,
    ) -> PyResult<MinimizationResult> {
        let Integer(max_evaluations) = max_evaluations;
        let JsonInput(value) = value;
        let options = json!({"maxEvaluations":max_evaluations,"preserveSchema":preserve_schema});
        let mut failure = None;
        let result = self.session.minimize(value.to_string(), options.to_string(), &mut |value| {
            let result = to_python(py, value)
                .and_then(|value| is_failure.call1((value,)))
                .and_then(|value| {
                    if !value.is_instance_of::<PyBool>() {
                        return Err(PyValueError::new_err("Failure oracle must return a boolean."));
                    }
                    value.extract::<bool>()
                });
            result.map_err(|error| {
                let message = error.to_string();
                failure = Some(error);
                riff_core::Error::Provider(message)
            })
        });
        if let Some(error) = failure {
            return Err(error);
        }
        let result = serde_json::from_str(&result.map_err(python_error)?)
            .map_err(|error| python_error(error.into()))?;
        MinimizationResult::from_result(py, &result)
    }

    fn is_valid(&self, value: JsonInput) -> PyResult<bool> {
        let JsonInput(value) = value;
        self.session.is_valid(value.to_string()).map_err(python_error)
    }
}

#[pyfunction]
pub fn model_is_valid(factory: &FixtureFactory, model: &Bound<'_, PyAny>) -> PyResult<bool> {
    let keywords = PyDict::new(model.py());
    keywords.set_item("mode", "json")?;
    keywords.set_item("by_alias", true)?;
    keywords.set_item("exclude_unset", true)?;
    keywords.set_item("round_trip", true)?;
    factory.is_valid(model.call_method("model_dump", (), Some(&keywords))?.extract::<JsonInput>()?)
}
