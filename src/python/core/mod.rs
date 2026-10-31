//! The `raydriver.core` Python submodule: shared device primitives.

pub(crate) mod mock;
pub(crate) mod types;

use pyo3::prelude::*;

pub(crate) const MODULE_DOC: &str = "\
Shared device primitives: transports, value types and session \
events. The protocol namespaces (raydriver.grbl, raydriver.smoothie, \
raydriver.marlin) build on these.
";

pyo3_stub_gen::module_doc!("raydriver.core", "{}", MODULE_DOC);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let core_mod = PyModule::new(py, "core")?;

    core_mod.setattr("__doc__", MODULE_DOC)?;
    core_mod.add("__all__", vec!["SessionEvent", "types", "transport"])?;
    core_mod.add_class::<crate::python::events::SessionEvent>()?;

    let transport_mod = PyModule::new(py, "transport")?;
    transport_mod.setattr(
        "__doc__",
        "Byte transports to devices: the mock transport for tests.",
    )?;
    transport_mod.add("__all__", vec!["MockTransport"])?;
    transport_mod.add_class::<mock::MockTransport>()?;
    core_mod.add_submodule(&transport_mod)?;

    let types_mod = PyModule::new(py, "types")?;
    types::register(&types_mod)?;
    core_mod.add_submodule(&types_mod)?;

    m.add_submodule(&core_mod)?;
    // Note: raydriver.core is intentionally NOT registered in
    // sys.modules — the Python __init__.py at
    // python/raydriver/core/__init__.py serves as the package and
    // delegates to this Rust module.

    Ok(())
}
