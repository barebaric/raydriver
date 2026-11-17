//! The `raydriver.xtool` Python submodule: sessions for xTool's
//! D-series HTTP API and M2 WS-V2 API.

pub(crate) mod m2_session;
pub(crate) mod session;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
xTool device drivers implemented in Rust.
XtoolSession supports D-series G-code jobs over the HTTP API. \
XtoolM2Session supports M2 WS-V2 monitoring, motion and job control; \
it does not upload G-code jobs. The M2 session requires explicit \
configuration to accept an untrusted local device certificate.
";

pyo3_stub_gen::module_doc!("raydriver.xtool", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let xtool_mod = PyModule::new(py, "xtool")?;

    xtool_mod.setattr("__doc__", MODULE_DOC)?;
    xtool_mod.add("__all__", vec!["XtoolSession", "XtoolM2Session"])?;
    xtool_mod.add_class::<session::XtoolSession>()?;
    xtool_mod.add_class::<m2_session::XtoolM2Session>()?;

    m.add_submodule(&xtool_mod)?;
    // Note: raydriver.xtool is intentionally NOT registered in
    // sys.modules — the Python __init__.py at
    // python/raydriver/xtool/__init__.py serves as the package and
    // delegates to this Rust module.

    Ok(())
}
