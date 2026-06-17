use crate::error::NativeError;
use crate::ladybug::{self, LadybugWriteRequest};
use crate::protocol::NativeSyntaxMaterializationRequest;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::time::Instant;

#[pyfunction]
fn materialize_syntax_batch(payload: &str) -> PyResult<String> {
    let decode_started = Instant::now();
    let request: NativeSyntaxMaterializationRequest =
        serde_json::from_str(payload).map_err(to_py_error)?;
    let json_decode_seconds = decode_started.elapsed().as_secs_f64();
    let mut response = crate::materialize_syntax_batch(request.clone()).map_err(to_py_error)?;
    response.add_phase_timing("rust_json_decode_seconds", json_decode_seconds);
    if !response.skipped {
        let database_write_started = Instant::now();
        ladybug::write_database_for_python(LadybugWriteRequest {
            db_path: request.db_path,
            include_fts: request.include_fts,
            schema_statements: request.schema_statements,
            copy_statements: response.copy_statements.clone(),
        })?;
        response.add_phase_timing(
            "database_write_seconds",
            database_write_started.elapsed().as_secs_f64(),
        );
        response.database_written = true;
    }
    let encode_started = Instant::now();
    let _ = serde_json::to_string(&response).map_err(to_py_error)?;
    response.add_phase_timing(
        "rust_json_encode_seconds",
        encode_started.elapsed().as_secs_f64(),
    );
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
