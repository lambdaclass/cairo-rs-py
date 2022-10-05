use crate::ids::PyIds;
use crate::pycell;
use crate::{
    memory::PyMemory, memory_segments::PySegmentManager, relocatable::PyRelocatable,
    utils::to_vm_error,
};
use cairo_rs::hint_processor::hint_processor_definition::HintProcessor;
use cairo_rs::hint_processor::proxies::exec_scopes_proxy::{ExecutionScopesProxy, get_exec_scopes_proxy};
use cairo_rs::hint_processor::proxies::vm_proxy::get_vm_proxy;
use cairo_rs::types::exec_scope::ExecutionScopes;
use cairo_rs::any_box;
use cairo_rs::vm::vm_core::VirtualMachine;
use cairo_rs::{
    hint_processor::builtin_hint_processor::builtin_hint_processor_definition::HintProcessorData,
    vm::errors::vm_errors::VirtualMachineError,
};
use num_bigint::BigInt;
use pyo3::PyCell;
use pyo3::{pyclass, pymethods, PyObject, ToPyObject};
use pyo3::{types::PyDict, Python};
use std::any::Any;
use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};

#[pyclass(unsendable)]
pub struct PyVM {
    pub(crate) vm: Rc<RefCell<VirtualMachine>>,
}

#[pymethods]
impl PyVM {
    #[new]
    pub fn new(prime: BigInt, trace_enabled: bool) -> PyVM {
        PyVM {
            vm: Rc::new(RefCell::new(VirtualMachine::new(prime, trace_enabled))),
        }
    }
}

impl PyVM {
    pub(crate) fn get_vm(&self) -> Rc<RefCell<VirtualMachine>> {
        Rc::clone(&self.vm)
    }

    pub(crate) fn execute_hint(
        &self,
        hint_data: &HintProcessorData,
        hint_locals: &HashMap<String, PyObject>,
        exec_scopes: &mut ExecutionScopesProxy,
    ) -> Result<(), VirtualMachineError> {
        Python::with_gil(|py| -> Result<(), VirtualMachineError> {
            let memory = PyMemory::new(&self);
            let segments = PySegmentManager::new(&self);
            let ap = PyRelocatable::from(self.vm.borrow().get_ap());
            let fp = PyRelocatable::from(self.vm.borrow().get_fp());
            let ids = PyIds::new(&self, &hint_data.ids_data, &hint_data.ap_tracking);

            let locals = get_scope_locals(exec_scopes, py)?;

            let globals = PyDict::new(py);

            globals
                .set_item("memory", pycell!(py, memory))
                .map_err(to_vm_error)?;
            globals
                .set_item("segments", pycell!(py, segments))
                .map_err(to_vm_error)?;
            globals
                .set_item("ap", pycell!(py, ap))
                .map_err(to_vm_error)?;
            globals
                .set_item("fp", pycell!(py, fp))
                .map_err(to_vm_error)?;
            globals
                .set_item("ids", pycell!(py, ids))
                .map_err(to_vm_error)?;

            for (name, pyobj) in hint_locals.iter(){
                globals.set_item(name, pyobj).map_err(to_vm_error)?;
            }

            py.run(&hint_data.code, Some(globals), Some(locals))
                .map_err(to_vm_error)?;

            update_scope_locals(exec_scopes, locals, py);

            Ok(())
        })?;

        Ok(())
    }

    pub(crate) fn step_hint(
        &self,
        hint_executor: &dyn HintProcessor,
        hint_locals: &HashMap<String, PyObject>,
        exec_scopes: &mut ExecutionScopes,
        hint_data_dictionary: &HashMap<usize, Vec<Box<dyn Any>>>,
    ) -> Result<(), VirtualMachineError> {
        if let Some(hint_list) = hint_data_dictionary.get(&self.vm.borrow().run_context.pc.offset) {
            let mut vm = self.vm.borrow_mut();
            let mut vm_proxy = get_vm_proxy(&mut vm);

            for hint_data in hint_list.iter() {
                //We create a new proxy with every hint as the current scope can change
                let mut exec_scopes_proxy = get_exec_scopes_proxy(exec_scopes);

                match hint_executor.execute_hint(&mut vm_proxy, &mut exec_scopes_proxy, hint_data) {
                    // if the hint is unknown to the builtin hint processor, use the execute_hint method from PyVM.
                    Err(VirtualMachineError::UnknownHint(_)) => {
                        let hint_data = hint_data
                        .downcast_ref::<HintProcessorData>()
                        .ok_or(VirtualMachineError::WrongHintData)?;

                    self.execute_hint(hint_data, hint_locals, &mut exec_scopes_proxy)?;
                    },
                    // if there is any other error, return that error
                    Err(e) => return Err(e),
                    Ok(_) => {},
                }
            }
        }

        Ok(())
    }

    pub(crate) fn step(
        &self,
        hint_executor: &dyn HintProcessor,
        hint_locals: &HashMap<String, PyObject>,
        exec_scopes: &mut ExecutionScopes,
        hint_data_dictionary: &HashMap<usize, Vec<Box<dyn Any>>>,
    ) -> Result<(), VirtualMachineError> {
        self.step_hint(hint_executor, hint_locals, exec_scopes, hint_data_dictionary)?;
        self.vm.borrow_mut().step_instruction()
    }
}

pub(crate) fn get_scope_locals<'a>(
    exec_scopes: &ExecutionScopesProxy,
    py: Python<'a>,
) -> Result<&'a PyDict, VirtualMachineError> {
    let locals = PyDict::new(py);
    for (name, elem) in exec_scopes.get_local_variables()? {
        if let Some(pyobj) = elem.downcast_ref::<PyObject>() {
            locals.set_item(name, pyobj).map_err(to_vm_error)?;
        }
    }
    Ok(locals)
}

pub(crate) fn update_scope_locals(
    exec_scopes: &mut ExecutionScopesProxy,
    locals: &PyDict,
    py: Python,
) {
    for (name, elem) in locals {
        exec_scopes.assign_or_update_variable(&name.to_string(), any_box!(elem.to_object(py)));
    }
}

#[cfg(test)]
mod test {
    use crate::vm_core::PyVM;
    use cairo_rs::{
        bigint,
        hint_processor::{
            builtin_hint_processor::builtin_hint_processor_definition::{
                BuiltinHintProcessor, HintProcessorData,
            },
            hint_processor_definition::HintReference,
            proxies::exec_scopes_proxy::get_exec_scopes_proxy,
        },
        types::{
            exec_scope::ExecutionScopes,
            relocatable::{MaybeRelocatable, Relocatable},
        },
    };
    use num_bigint::{BigInt, Sign};
    use std::collections::HashMap;

    #[test]
    fn execute_print_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let code = "print(ap)";
        let hint_data = HintProcessorData::new_default(code.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(
                &hint_data,
                &HashMap::new(),
                &mut get_exec_scopes_proxy(&mut ExecutionScopes::new())
            ),
            Ok(())
        );
    }

    #[test]
    fn set_memory_item_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let code = "print(ap)";
        let hint_data = HintProcessorData::new_default(code.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(
                &hint_data,
                &HashMap::new(),
                &mut get_exec_scopes_proxy(&mut ExecutionScopes::new())
            ),
            Ok(())
        );
    }

    #[test]
    fn ids_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        for _ in 0..2 {
            vm.vm.borrow_mut().add_memory_segment();
        }
        let references = HashMap::from([
            (String::from("a"), HintReference::new_simple(2)),
            (String::from("b"), HintReference::new_simple(1)),
        ]);
        vm.vm
            .borrow_mut()
            .memory
            .insert(
                &Relocatable::from((1, 1)),
                &MaybeRelocatable::from(bigint!(2)),
            )
            .unwrap();
        let code = "ids.a = ids.b";
        let hint_data = HintProcessorData::new_default(code.to_string(), references);
        assert_eq!(
            vm.execute_hint(
                &hint_data,
                &HashMap::new(),
                &mut get_exec_scopes_proxy(&mut ExecutionScopes::new())
            ),
            Ok(())
        );
        assert_eq!(
            vm.vm.borrow().memory.get(&Relocatable::from((1, 2))),
            Ok(Some(&MaybeRelocatable::from(bigint!(2))))
        );
    }

    #[test]
    // This test is analogous to the `test_step_for_preset_memory` unit test in the cairo-rs crate.
    fn test_step_with_no_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );

        for _ in 0..2 {
            vm.vm.borrow_mut().add_memory_segment();
        }

        let hint_processor = BuiltinHintProcessor::new_empty();

        vm.vm.borrow_mut().run_context.pc = Relocatable::from((0, 0));
        vm.vm.borrow_mut().run_context.ap = 2usize;
        vm.vm.borrow_mut().run_context.fp = 2usize;

        vm.vm
            .borrow_mut()
            .insert_value(&Relocatable::from((0, 0)), bigint!(2345108766317314046_u64))
            .unwrap();
        vm.vm
            .borrow_mut()
            .insert_value(&Relocatable::from((1, 0)), &Relocatable::from((2, 0)))
            .unwrap();
        vm.vm
            .borrow_mut()
            .insert_value(&Relocatable::from((1, 1)), &Relocatable::from((3, 0)))
            .unwrap();

        assert_eq!(
            vm.step(
                &hint_processor,
                &HashMap::new(),
                &mut ExecutionScopes::new(),
                &HashMap::new()
            ),
            Ok(())
        );
    }

    #[test]
    fn test_step_with_print_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );

        for _ in 0..2 {
            vm.vm.borrow_mut().add_memory_segment();
        }

        let hint_processor = BuiltinHintProcessor::new_empty();

        vm.vm.borrow_mut().run_context.pc = Relocatable::from((0, 0));
        vm.vm.borrow_mut().run_context.ap = 2usize;
        vm.vm.borrow_mut().run_context.fp = 2usize;

        vm.vm
            .borrow_mut()
            .insert_value(&Relocatable::from((0, 0)), bigint!(2345108766317314046_u64))
            .unwrap();
        vm.vm
            .borrow_mut()
            .insert_value(&Relocatable::from((1, 0)), &Relocatable::from((2, 0)))
            .unwrap();
        vm.vm
            .borrow_mut()
            .insert_value(&Relocatable::from((1, 1)), &Relocatable::from((3, 0)))
            .unwrap();

        let code = "print(ap)";
        let hint_proc_data = HintProcessorData::new_default(code.to_string(), HashMap::new());

        let mut hint_data = HashMap::new();
        hint_data.insert(0, hint_proc_data);

        assert_eq!(
            vm.step(
                &hint_processor,
                &HashMap::new(),
                &mut ExecutionScopes::new(),
                &HashMap::new()
            ),
            Ok(())
        );
    }

    #[test]
    fn scopes_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        for _ in 0..2 {
            vm.vm.borrow_mut().add_memory_segment();
        }

        let mut exec_scopes = ExecutionScopes::new();
        let exec_scopes_proxy = &mut get_exec_scopes_proxy(&mut exec_scopes);
        let code_a = "num = 6";
        let code_b = "assert(num == 6)";
        let hint_data = HintProcessorData::new_default(code_a.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data,&HashMap::new(), exec_scopes_proxy), Ok(()));
        let hint_data = HintProcessorData::new_default(code_b.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data,&HashMap::new(), exec_scopes_proxy), Ok(()));
    }
}
