native mod rustrt {
    fn rust_dbg_call(cb: *u8,
                     data: ctypes::uintptr_t) -> ctypes::uintptr_t;
}

crust fn cb(data: ctypes::uintptr_t) -> ctypes::uintptr_t {
    if data == 1u {
        data
    } else {
        count(data - 1u) + count(data - 1u)
    }
}

fn count(n: uint) -> uint {
    task::yield();
    rustrt::rust_dbg_call(cb, n)
}

fn main() {
    iter::repeat(10u) {||
        task::spawn {||
            let result = count(5u);
            #debug("result = %?", result);
            assert result == 16u;
        };
    }
}
