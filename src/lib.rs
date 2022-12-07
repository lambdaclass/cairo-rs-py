pub mod cairo_run;
pub mod cairo_runner;
mod ecdsa;
pub mod ids;
mod memory;
mod memory_segments;
mod range_check;
mod relocatable;
mod run_context;
mod scope_manager;
mod to_felt_or_relocatable;
mod utils;
mod vm_core;

use cairo_runner::PyCairoRunner;
use pyo3::prelude::*;

#[pymodule]
fn cairo_rs_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyCairoRunner>()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use pyo3::prelude::*;
    use pyo3::Python;

    #[test]
    fn cairo_rs_py_test() {
        Python::with_gil(|py| {
            let module = PyModule::new(py, "My Module");
            let result = crate::cairo_rs_py(py, module.unwrap());
            match result {
                Ok(_v) => assert!(true),
                Err(e) => assert!(false, "error: {e:?}"),
            }
        });
    }
}
