//! The `raydriver.marlin` Python submodule.

pub(crate) mod parser;
pub(crate) mod session;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
Marlin device session: strict ping-pong commands, M114 position \
polling. Dialects remain data owned by Rayforge: resolved command \
templates are passed into the session.
";

pyo3_stub_gen::module_doc!("raydriver.marlin", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let marlin_mod = PyModule::new(py, "marlin")?;

    marlin_mod.setattr("__doc__", MODULE_DOC)?;
    marlin_mod.add("__all__", vec!["MarlinSession", "parser"])?;
    marlin_mod.add_class::<session::MarlinSession>()?;

    let parser_mod = PyModule::new(py, "parser")?;
    parser::register(&parser_mod)?;
    marlin_mod.add_submodule(&parser_mod)?;

    m.add_submodule(&marlin_mod)?;
    Ok(())
}
