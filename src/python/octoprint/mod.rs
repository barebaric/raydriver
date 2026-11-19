//! The `raydriver.octoprint` Python submodule.

pub(crate) mod probe;
pub(crate) mod session;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
OctoPrint server support implemented in Rust. OctoPrintSession \
submits G-code jobs over the server's REST API and tracks printer \
state via its SockJS WebSocket (with REST polling fallback).
";

pyo3_stub_gen::module_doc!("raydriver.octoprint", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let octoprint_mod = PyModule::new(py, "octoprint")?;

    octoprint_mod.setattr("__doc__", MODULE_DOC)?;
    octoprint_mod.add("__all__", vec!["OctoPrintSession", "probe"])?;
    probe::register(&octoprint_mod)?;
    octoprint_mod.add_class::<session::OctoPrintSession>()?;

    m.add_submodule(&octoprint_mod)?;
    // Note: raydriver.octoprint is intentionally NOT registered in
    // sys.modules — the Python __init__.py at
    // python/raydriver/octoprint/__init__.py serves as the package
    // and delegates to this Rust module.

    Ok(())
}
