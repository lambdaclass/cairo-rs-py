#[cfg(test)]
mod test {
    use num_bigint::{BigInt, Sign};

    use crate::vm_core::PyVM;

    #[test]
    fn cairo_run_fibonacci() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        vm.cairo_run_py(
            "cairo_programs/fibonacci.json",
            "main",
            false,
            None,
            None,
            None,
        )
        .expect("Couldn't run program");
    }

    #[test]
    fn cairo_run_array_sum() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        vm.cairo_run_py(
            "cairo_programs/array_sum.json",
            "main",
            false,
            None,
            None,
            None,
        )
        .expect("Couldn't run program");
    }

    #[test]
    fn cairo_run_hint_print_vars() {
        let vm = PyVM::new(
            BigInt::new(Sign::Plus, vec![1, 0, 0, 0, 0, 0, 17, 134217728]),
            false,
        );
        vm.cairo_run_py(
            "cairo_programs/hint_print_vars.json",
            "main",
            false,
            None,
            None,
            None,
        )
        .expect("Couldn't run program");
    }

    #[test]
    fn cairo_run_add_segment() {
        cairo_run::cairo_run_py(
            "cairo_programs/memory_add.json",
            "main",
            false,
            false,
            None,
            None,
            None,
        )
        .expect("Couldn't run program");
    }

    #[test]
    fn cairo_run_assert_not_zero() {
        cairo_run::cairo_run_py(
            "cairo_programs/assert_not_zero.json",
            "main",
            false,
            false,
            None,
            None,
            None,
        )
        .expect("Couldn't run program");
    }
}
