use crate::ids::PyIds;
use crate::pycell;
use crate::scope_manager::{PyEnterScope, PyExitScope};
use crate::{
    memory::PyMemory, memory_segments::PySegmentManager, relocatable::PyRelocatable,
    utils::to_vm_error,
};
use cairo_rs::any_box;
use cairo_rs::hint_processor::hint_processor_definition::HintProcessor;
use cairo_rs::types::exec_scope::ExecutionScopes;
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
        hint_locals: &mut HashMap<String, PyObject>,
        exec_scopes: &mut ExecutionScopes,
    ) -> Result<(), VirtualMachineError> {
        Python::with_gil(|py| -> Result<(), VirtualMachineError> {
            let memory = PyMemory::new(self);
            let segments = PySegmentManager::new(self);
            let ap = PyRelocatable::from(self.vm.borrow().get_ap());
            let fp = PyRelocatable::from(self.vm.borrow().get_fp());
            let ids = PyIds::new(self, &hint_data.ids_data, &hint_data.ap_tracking);
            let enter_scope = pycell!(py, PyEnterScope::new());
            let exit_scope = pycell!(py, PyExitScope::new());

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

            globals
                .set_item("vm_enter_scope", enter_scope)
                .map_err(to_vm_error)?;
            globals
                .set_item("vm_exit_scope", exit_scope)
                .map_err(to_vm_error)?;

            for (name, pyobj) in hint_locals.iter() {
                locals.set_item(name, pyobj).map_err(to_vm_error)?;
            }

            py.run(&hint_data.code, Some(globals), Some(locals))
                .map_err(to_vm_error)?;

            update_scope_hint_locals(exec_scopes, hint_locals, locals, py);

            enter_scope.borrow().update_scopes(exec_scopes)?;
            exit_scope.borrow().update_scopes(exec_scopes)
        })?;

        Ok(())
    }

    pub(crate) fn step_hint(
        &self,
        hint_executor: &dyn HintProcessor,
        hint_locals: &mut HashMap<String, PyObject>,
        exec_scopes: &mut ExecutionScopes,
        hint_data_dictionary: &HashMap<usize, Vec<Box<dyn Any>>>,
    ) -> Result<(), VirtualMachineError> {
        let pc_offset = self.vm.borrow().get_pc().offset;

        if let Some(hint_list) = hint_data_dictionary.get(&pc_offset) {
            for hint_data in hint_list.iter() {
                if self.should_run_py_hint(hint_executor, exec_scopes, hint_data)? {
                    let hint_data = hint_data
                        .downcast_ref::<HintProcessorData>()
                        .ok_or(VirtualMachineError::WrongHintData)?;

                    self.execute_hint(hint_data, hint_locals, exec_scopes)?;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn step(
        &self,
        hint_executor: &dyn HintProcessor,
        hint_locals: &mut HashMap<String, PyObject>,
        exec_scopes: &mut ExecutionScopes,
        hint_data_dictionary: &HashMap<usize, Vec<Box<dyn Any>>>,
    ) -> Result<(), VirtualMachineError> {
        self.step_hint(
            hint_executor,
            hint_locals,
            exec_scopes,
            hint_data_dictionary,
        )?;
        self.vm.borrow_mut().step_instruction()
    }

    fn should_run_py_hint(
        &self,
        hint_executor: &dyn HintProcessor,
        exec_scopes: &mut ExecutionScopes,
        hint_data: &Box<dyn Any>,
    ) -> Result<bool, VirtualMachineError> {
        let mut vm = self.vm.borrow_mut();
        match hint_executor.execute_hint(&mut vm, exec_scopes, hint_data) {
            Ok(()) => Ok(false),
            Err(VirtualMachineError::UnknownHint(_)) => Ok(true),
            Err(e) => Err(e),
        }
    }
}

pub(crate) fn get_scope_locals<'a>(
    exec_scopes: &ExecutionScopes,
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

pub(crate) fn update_scope_hint_locals(
    exec_scopes: &mut ExecutionScopes,
    hint_locals: &mut HashMap<String, PyObject>,
    locals: &PyDict,
    py: Python,
) {
    for (name, elem) in locals {
        let name = name.to_string();
        if hint_locals.keys().cloned().any(|x| x == name) {
            hint_locals.insert(name, elem.to_object(py));
        } else {
            exec_scopes.assign_or_update_variable(&name, any_box!(elem.to_object(py)));
        }
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
        },
        types::{
            exec_scope::ExecutionScopes,
            relocatable::{MaybeRelocatable, Relocatable},
        },
        vm::errors::{exec_scope_errors::ExecScopeError, vm_errors::VirtualMachineError},
    };
    use num_bigint::{BigInt, Sign};
    use pyo3::{PyObject, Python, ToPyObject};
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
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut ExecutionScopes::new()),
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
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut ExecutionScopes::new()),
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
            .insert_value(
                &Relocatable::from((1, 1)),
                &MaybeRelocatable::from(bigint!(2usize)),
            )
            .unwrap();
        let code = "ids.a = ids.b";
        let hint_data = HintProcessorData::new_default(code.to_string(), references);
        assert_eq!(
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut ExecutionScopes::new()),
            Ok(())
        );
        assert_eq!(
            vm.vm.borrow().get_maybe(&Relocatable::from((1, 2))),
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

        vm.vm.borrow_mut().set_pc(Relocatable::from((0, 0)));
        vm.vm.borrow_mut().set_ap(2);
        vm.vm.borrow_mut().set_fp(2);

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
                &mut HashMap::new(),
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

        vm.vm.borrow_mut().set_pc(Relocatable::from((0, 0)));
        vm.vm.borrow_mut().set_ap(2);
        vm.vm.borrow_mut().set_fp(2);

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
                &mut HashMap::new(),
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
        let code_a = "num = 6";
        let code_b = "assert(num == 6)";
        let hint_data = HintProcessorData::new_default(code_a.to_string(), HashMap::new());

        assert_eq!(vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes), Ok(()));
        let hint_data = HintProcessorData::new_default(code_b.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes), Ok(()));
    }

    #[test]
    fn scopes_hint_modify() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        for _ in 0..2 {
            vm.vm.borrow_mut().add_memory_segment();
        }

        let mut exec_scopes = ExecutionScopes::new();
        let code_a = "num = 6";
        let code_b = "assert(num == 6)";
        let code_c = "num = num + 3";
        let code_d = "assert(num == 9)";
        let hint_data = HintProcessorData::new_default(code_a.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes),
            Ok(())
        );
        let hint_data = HintProcessorData::new_default(code_b.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes),
            Ok(())
        );
        let hint_data = HintProcessorData::new_default(code_c.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes),
            Ok(())
        );
        let hint_data = HintProcessorData::new_default(code_d.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes),
            Ok(())
        );
    }

    #[test]
    fn modify_hint_locals() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let code = "word = word[::-1]
print(word)";
        let hint_data = HintProcessorData::new_default(code.to_string(), HashMap::new());
        let word = Python::with_gil(|py| -> PyObject { "fruity".to_string().to_object(py) });
        let mut hint_locals = HashMap::from([("word".to_string(), word)]);
        assert_eq!(
            vm.execute_hint(
                &hint_data,
                &mut hint_locals,
                &mut ExecutionScopes::new()
            ),
            Ok(())
        );
        let word_res = Python::with_gil(|py| -> String {
            hint_locals
                .get("word")
                .unwrap()
                .extract::<String>(py)
                .unwrap()
        });
        assert_eq!(word_res, "ytiurf".to_string())
    }

    #[test]
    fn exit_main_scope_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let mut exec_scopes = ExecutionScopes::new();
        let code = "vm_exit_scope()";
        let hint_data = HintProcessorData::new_default(code.to_string(), HashMap::new());
        assert_eq!(
            vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes),
            Err(VirtualMachineError::MainScopeError(
                ExecScopeError::ExitMainScopeError
            ))
        );
    }

    #[test]
    fn enter_scope_empty_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let mut exec_scopes = ExecutionScopes::new();
        let code = "vm_enter_scope()";
        let hint_data = HintProcessorData::new_default(code.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes), Ok(()));
        assert_eq!(exec_scopes.data.len(), 2)
    }

    #[test]
    fn enter_exit_scope_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let mut exec_scopes = ExecutionScopes::new();
        let code = "vm_enter_scope()
vm_exit_scope()";
        let hint_data = HintProcessorData::new_default(code.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes), Ok(()));
        assert_eq!(exec_scopes.data.len(), 1)
    }

    #[test]
    fn enter_scope_non_empty_hint() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        let mut exec_scopes = ExecutionScopes::new();
        let code_a = "vm_enter_scope({'n': 12})";
        let code_b = "assert(n == 12)";
        let hint_data = HintProcessorData::new_default(code_a.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes), Ok(()));
        let hint_data = HintProcessorData::new_default(code_b.to_string(), HashMap::new());
        assert_eq!(vm.execute_hint(&hint_data, &mut HashMap::new(), &mut exec_scopes), Ok(()));
        assert_eq!(exec_scopes.data.len(), 2);
        assert!(exec_scopes.data[0].is_empty());
    }
}
