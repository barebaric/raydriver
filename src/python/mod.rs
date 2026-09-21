//! PyO3 bindings for the GRBL protocol stack.

pub(crate) mod events;
pub(crate) mod mock;
pub(crate) mod parser;
pub(crate) mod pyfuture;
pub(crate) mod runtime;
pub(crate) mod session;
pub(crate) mod types;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
GRBL device drivers implemented in Rust.

The central type is GrblSession: it owns the connection lifecycle \
(handshake, status polling, reconnect), interactive commands, and \
job streaming with character-counting flow control, stall detection \
and deadlock recovery. Dialects remain data owned by Rayforge: \
resolved command templates are passed into the session.
";

pyo3_stub_gen::module_doc!("raydriver.grbl", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let grbl_mod = PyModule::new(py, "grbl")?;

    grbl_mod.setattr("__doc__", MODULE_DOC)?;
    grbl_mod.add("__all__", vec!["GrblSession", "MockTransport", "types"])?;

    grbl_mod.add_class::<session::GrblSession>()?;
    grbl_mod.add_class::<mock::MockTransport>()?;

    let types_mod = PyModule::new(py, "types")?;
    types::register(&types_mod)?;
    grbl_mod.add_submodule(&types_mod)?;

    let parser_mod = PyModule::new(py, "parser")?;
    parser::register(&parser_mod)?;
    grbl_mod.add_submodule(&parser_mod)?;

    m.add_submodule(&grbl_mod)?;
    // Note: raydriver.grbl is intentionally NOT registered in
    // sys.modules — the Python __init__.py at
    // python/raydriver/grbl/__init__.py serves as the package and
    // delegates to this Rust module.

    Ok(())
}
