import cairo_rs_py

def new_vm():
    return cairo_rs_py.PyVM(3618502788666131213697322783095070105623107215331596699973092056135872020481, True)

def test_program(program_name: str):
    print(new_vm().cairo_run(f"cairo_programs/{program_name}.json", "main", False))

if __name__ == "__main__":
    test_program("assert_not_zero")
    test_program("memory_add")
    test_program("hint_print_vars")
    test_program("vm_scope_hints")
    test_program("is_le_felt_hint")
    test_program("assert_le_felt_hint")
    test_program("is_nn_hint")
    test_program("ec_mul_inner")
    # test_program("ec_negate")
    test_program("assert_nn_hint")
    # test_program("reduce_and_nondet_bigint3")
    # test_program("is_zero")
    # test_program("div_mod_n")
    # test_program("get_point_from_x")
    # test_program("compute_slope")
    # test_program("ec_doble")
    test_program("memcpy")
    test_program("memset")
    # test_program("keccak")
    # test_program("_keccak")
    test_program("keccak_add_uint256")
    # test_program("keccak_copy_inputs")
    test_program("unsafe_keccak")
    # test_program("unsafe_keccak_finalize")

    print("\nAll test have passed")
