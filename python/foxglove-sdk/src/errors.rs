use foxglove::FoxgloveError;
use pyo3::{exceptions::PyRuntimeError, PyErr};
use thiserror::Error;

/// Expose underlying `FoxgloveError`s from the Rust SDK to `PyErr`s from pyo3
#[derive(Error, Debug)]
#[error(transparent)]
pub struct PyFoxgloveError(#[from] FoxgloveError);

impl From<PyFoxgloveError> for PyErr {
    fn from(err: PyFoxgloveError) -> PyErr {
        PyRuntimeError::new_err(format!("FoxgloveError: {err}"))
    }
}
