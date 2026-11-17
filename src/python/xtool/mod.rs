//! The `raydriver.xtool` Python submodule: the xTool WiFi-API
//! session.

pub(crate) mod session;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
xTool device drivers implemented in Rust.

The central type is XtoolSession: it owns the connection lifecycle \
of xTool's proprietary WiFi API (HTTP command exchange, WebSocket \
job-state events, job upload and execution). Dialects remain data \
owned by Rayforge: resolved power-command templates are passed \
into the session.
";

pyo3_stub_gen::module_doc!("raydriver.xtool", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let xtool_mod = PyModule::new(py, "xtool")?;

    xtool_mod.setattr("__doc__", MODULE_DOC)?;
    xtool_mod.add("__all__", vec!["XtoolSession"])?;
    xtool_mod.add_class::<session::XtoolSession>()?;

    m.add_submodule(&xtool_mod)?;
    // Note: raydriver.xtool is intentionally NOT registered in
    // sys.modules — the Python __init__.py at
    // python/raydriver/xtool/__init__.py serves as the package and
    // delegates to this Rust module.

    Ok(())
}
