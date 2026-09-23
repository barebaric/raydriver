//! Root PyO3 module.

pub(crate) mod events;
pub(crate) mod grbl;
pub(crate) mod pyfuture;
pub(crate) mod runtime;

use pyo3::prelude::*;

/// Register the root `raydriver` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    grbl::register(m)?;
    Ok(())
}
