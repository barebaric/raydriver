//! The `raydriver.xtool` Python submodule: sessions for xTool's
//! D-series HTTP API and M2 WS-V2 API.

pub(crate) mod m2;
pub(crate) mod session;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
xTool device drivers implemented in Rust.
XtoolSession supports D-series G-code jobs over the HTTP API. M2Session \
is available at raydriver.xtool.m2 for WS-V2 monitoring and control; \
it does not upload G-code jobs. Its untrusted-certificate option is \
explicit and disabled by default.
";

pyo3_stub_gen::module_doc!("raydriver.xtool", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let xtool_mod = PyModule::new(py, "xtool")?;

    xtool_mod.setattr("__doc__", MODULE_DOC)?;
    xtool_mod.add("__all__", vec!["XtoolSession", "m2"])?;
    xtool_mod.add_class::<session::XtoolSession>()?;
    m2::register(&xtool_mod)?;

    m.add_submodule(&xtool_mod)?;
    // Note: raydriver.xtool is intentionally NOT registered in
    // sys.modules — the Python __init__.py at
    // python/raydriver/xtool/__init__.py serves as the package and
    // delegates to this Rust module.

    Ok(())
}
