//! PyO3 bindings for the xTool M2 session (`raydriver.xtool.m2`).

pub(crate) mod session;

use pyo3::prelude::*;

const MODULE_DOC: &str = "M2 WS-V2 driver.";

pyo3_stub_gen::module_doc!("raydriver.xtool.m2", "{}", MODULE_DOC);

pub(crate) fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let module = PyModule::new(py, "m2")?;
    module.setattr("__doc__", MODULE_DOC)?;
    module.add("__all__", vec!["M2Session"])?;
    module.add_class::<session::PyM2Session>()?;
    parent.add_submodule(&module)?;
    Ok(())
}
