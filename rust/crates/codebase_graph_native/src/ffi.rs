use crate::error::NativeError;
use crate::ladybug::{self, LadybugWriteRequest};
use crate::protocol::NativeSyntaxMaterializationRequest;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyfunction]
fn materialize_syntax_batch(payload: &str) -> PyResult<String> {
    let request: NativeSyntaxMaterializationRequest =
        serde_json::from_str(payload).map_err(to_py_error)?;
    let mut response = crate::materialize_syntax_batch(request.clone()).map_err(to_py_error)?;
    if !response.skipped {
        ladybug::write_database_for_python(LadybugWriteRequest {
            db_path: request.db_path,
            include_fts: request.include_fts,
            schema_statements: request.schema_statements,
            copy_statements: response.copy_statements.clone(),
        })?;
        response.database_written = true;
    }
    serde_json::to_string(&response).map_err(to_py_error)
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(materialize_syntax_batch, module)?)?;
    Ok(())
}

fn to_py_error(error: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

impl From<NativeError> for PyErr {
    fn from(error: NativeError) -> Self {
        to_py_error(error)
    }
}
