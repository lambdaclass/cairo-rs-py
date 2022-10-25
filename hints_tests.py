import cairo_rs_py

def new_runner(program_name: str):
    return cairo_rs_py.CairoRunner(f"cairo_programs/{program_name}.json", "main")

def test_program(program_name: str):
    print(new_runner(program_name).cairo_run(False))

if __name__ == "__main__":
    test_program("assert_not_zero")
    test_program("memory_add")
    test_program("hint_print_vars")
    test_program("vm_scope_hints")
    test_program("is_le_felt_hint")
    test_program("ec_mul_inner")
    # test_program("ec_negate") # FAILING (Issue with ids structs)
    test_program("assert_nn_hint")
    # test_program("pow") # FAILING (Issue with ids structs)
    test_program("is_nn")
    test_program("is_positive")
    test_program("assert_not_zero")
    test_program("assert_le_felt")
    test_program("assert_lt_felt")
    test_program("assert_not_equal")
    # test_program("reduce_and_nondet_bigint3")
    # test_program("is_zero")
    # test_program("div_mod_n")
    # test_program("get_point_from_x")
    # test_program("compute_slope")
    # test_program("ec_doble")
    test_program("memcpy")
    test_program("memset")
    print("\nAll test have passed")
