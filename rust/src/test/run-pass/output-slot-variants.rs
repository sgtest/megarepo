

fn ret_int_i() -> int { return 10; }

fn ret_ext_i() -> @int { return @10; }

fn ret_int_rec() -> {a: int, b: int} { return {a: 10, b: 10}; }

fn ret_ext_rec() -> @{a: int, b: int} { return @{a: 10, b: 10}; }

fn ret_ext_mem() -> {a: @int, b: @int} { return {a: @10, b: @10}; }

fn ret_ext_ext_mem() -> @{a: @int, b: @int} { return @{a: @10, b: @10}; }

fn main() {
    let mut int_i: int;
    let mut ext_i: @int;
    let mut int_rec: {a: int, b: int};
    let mut ext_rec: @{a: int, b: int};
    let mut ext_mem: {a: @int, b: @int};
    let mut ext_ext_mem: @{a: @int, b: @int};
    int_i = ret_int_i(); // initializing

    int_i = ret_int_i(); // non-initializing

    int_i = ret_int_i(); // non-initializing

    ext_i = ret_ext_i(); // initializing

    ext_i = ret_ext_i(); // non-initializing

    ext_i = ret_ext_i(); // non-initializing

    int_rec = ret_int_rec(); // initializing

    int_rec = ret_int_rec(); // non-initializing

    int_rec = ret_int_rec(); // non-initializing

    ext_rec = ret_ext_rec(); // initializing

    ext_rec = ret_ext_rec(); // non-initializing

    ext_rec = ret_ext_rec(); // non-initializing

    ext_mem = ret_ext_mem(); // initializing

    ext_mem = ret_ext_mem(); // non-initializing

    ext_mem = ret_ext_mem(); // non-initializing

    ext_ext_mem = ret_ext_ext_mem(); // initializing

    ext_ext_mem = ret_ext_ext_mem(); // non-initializing

    ext_ext_mem = ret_ext_ext_mem(); // non-initializing

}
