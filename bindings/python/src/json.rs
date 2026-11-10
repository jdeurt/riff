//! Strict Python JSON conversion at the native boundary.
use std::str::FromStr;

use num_bigint::BigInt;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use serde_json::{Map, Number, Value};

pub struct JsonInput(pub Value);

impl FromPyObject<'_, '_> for JsonInput {
    type Error = PyErr;
    fn extract(object: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
        read(&object, 0).map(Self)
    }
}

pub struct MappingInput(pub Map<String, Value>);
impl FromPyObject<'_, '_> for MappingInput {
    type Error = PyErr;
    fn extract(object: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
        let mut values = Map::new();
        for item in object.call_method0("items")?.try_iter()? {
            let (name, value): (String, JsonInput) = item?.extract()?;
            let _ = values.insert(name, value.0);
        }
        Ok(Self(values))
    }
}

pub struct SequenceInput(pub Vec<Value>);
impl FromPyObject<'_, '_> for SequenceInput {
    type Error = PyErr;
    fn extract(object: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
        object
            .try_iter()?
            .map(|item| item?.extract::<JsonInput>().map(|value| value.0))
            .collect::<PyResult<Vec<_>>>()
            .map(Self)
    }
}

pub struct ListInput<T>(pub Vec<T>);

impl<'py, T: for<'a> FromPyObject<'a, 'py, Error = PyErr>> FromPyObject<'_, 'py> for ListInput<T> {
    type Error = PyErr;
    fn extract(object: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        let list = object.extract::<Bound<'py, PyList>>()?;
        list.iter().map(|item| item.extract()).collect::<PyResult<Vec<_>>>().map(Self)
    }
}

pub struct Integer(pub Value);

impl Integer {
    pub fn small(value: u16) -> Self {
        Self(Value::from(value))
    }
}

impl FromPyObject<'_, '_> for Integer {
    type Error = PyErr;
    fn extract(object: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
        if !object.is_instance_of::<PyInt>() || object.is_instance_of::<PyBool>() {
            return Err(PyValueError::new_err(
                "Expected an integer, not a boolean or another type.",
            ));
        }
        let integer_type = object.py().get_type::<PyInt>();
        let bits = integer_type.getattr("bit_length")?.call1((&object,))?.extract::<u64>()?;
        if bits > riff_core::MAX_INTEGER_BITS {
            return Err(PyValueError::new_err(
                "Integer exceeds supported numeric allocation limits.",
            ));
        }
        // Normalize with the built-in descriptor before conversion can call subclass hooks.
        let integer: BigInt = integer_type.getattr("__int__")?.call1((&object,))?.extract()?;
        Number::from_str(&integer.to_string())
            .map(|number| Self(Value::Number(number)))
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }
}

fn read(object: &Bound<'_, PyAny>, depth: usize) -> PyResult<Value> {
    if depth > riff_core::MAX_INPUT_DEPTH {
        return Err(PyValueError::new_err("JSON input nesting exceeds 128 levels."));
    }
    if object.is_none() {
        return Ok(Value::Null);
    }
    if object.is_instance_of::<PyBool>() {
        return object.extract::<bool>().map(Value::Bool);
    }
    if object.is_instance_of::<PyInt>() {
        return object.extract::<Integer>().map(|integer| integer.0);
    }
    if object.is_instance_of::<PyFloat>() {
        return Number::from_f64(object.extract()?)
            .map(Value::Number)
            .ok_or_else(|| PyValueError::new_err("JSON numbers must be finite."));
    }
    if object.is_instance_of::<PyString>() {
        return object.extract::<String>().map(Value::String);
    }
    if object.is_instance_of::<PyList>() {
        let list = object.extract::<Bound<'_, PyList>>()?;
        let mut values = Vec::new();
        for item in list.iter() {
            values.push(read(&item, depth + 1)?);
        }
        return Ok(Value::Array(values));
    }
    if object.is_instance_of::<PyDict>() {
        let entries: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = object
            .call_method0("items")?
            .try_iter()?
            .map(|item| item?.extract())
            .collect::<PyResult<_>>()?;
        let mut values = Map::new();
        for (key, value) in entries {
            let name = key
                .extract::<String>()
                .map_err(|_| PyValueError::new_err("JSON object keys must be strings."))?;
            let _ = values.insert(name, read(&value, depth + 1)?);
        }
        return Ok(Value::Object(values));
    }
    Err(PyValueError::new_err("Expected JSON scalars, lists, or string-keyed dictionaries."))
}

pub fn to_python(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(value) => Ok(value.into_pyobject(py)?.to_owned().into_any().unbind()),
        Value::String(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
        Value::Number(number) => {
            let text = number.to_string();
            if !text.contains(['.', 'e', 'E']) {
                let integer = BigInt::from_str(&text)
                    .map_err(|error| PyValueError::new_err(error.to_string()))?;
                return Ok(integer.into_pyobject(py)?.into_any().unbind());
            }
            let float = number.as_f64().filter(|value| value.is_finite()).ok_or_else(|| {
                PyValueError::new_err("JSON number cannot be represented as a finite float.")
            })?;
            Ok(float.into_pyobject(py)?.into_any().unbind())
        }
        Value::Array(values) => {
            let items =
                values.iter().map(|value| to_python(py, value)).collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, items)?.into_any().unbind())
        }
        Value::Object(values) => {
            let dictionary = PyDict::new(py);
            for (name, value) in values {
                dictionary.set_item(name, to_python(py, value)?)?;
            }
            Ok(dictionary.into_any().unbind())
        }
    }
}
