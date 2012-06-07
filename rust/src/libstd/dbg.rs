#[doc = "Unsafe debugging functions for inspecting values."];

import unsafe::reinterpret_cast;

export debug_tydesc;
export debug_opaque;
export debug_box;
export debug_tag;
export debug_fn;
export ptr_cast;
export refcount;
export breakpoint;

#[abi = "cdecl"]
native mod rustrt {
    fn debug_tydesc(td: *sys::type_desc);
    fn debug_opaque(td: *sys::type_desc, x: *());
    fn debug_box(td: *sys::type_desc, x: *());
    fn debug_tag(td: *sys::type_desc, x: *());
    fn debug_fn(td: *sys::type_desc, x: *());
    fn debug_ptrcast(td: *sys::type_desc, x: *()) -> *();
    fn rust_dbg_breakpoint();
}

fn debug_tydesc<T>() {
    rustrt::debug_tydesc(sys::get_type_desc::<T>());
}

fn debug_opaque<T>(x: T) {
    rustrt::debug_opaque(sys::get_type_desc::<T>(), ptr::addr_of(x) as *());
}

fn debug_box<T>(x: @T) {
    rustrt::debug_box(sys::get_type_desc::<T>(), ptr::addr_of(x) as *());
}

fn debug_tag<T>(x: T) {
    rustrt::debug_tag(sys::get_type_desc::<T>(), ptr::addr_of(x) as *());
}

fn debug_fn<T>(x: T) {
    rustrt::debug_fn(sys::get_type_desc::<T>(), ptr::addr_of(x) as *());
}

unsafe fn ptr_cast<T, U>(x: @T) -> @U {
    reinterpret_cast(
        rustrt::debug_ptrcast(sys::get_type_desc::<T>(),
                              reinterpret_cast(x)))
}

fn refcount<T>(a: @T) -> uint unsafe {
    let p: *uint = unsafe::reinterpret_cast(a);
    ret *p;
}

#[doc = "Triggers a debugger breakpoint"]
fn breakpoint() {
    rustrt::rust_dbg_breakpoint();
}

#[test]
fn test_breakpoint_should_not_abort_process_when_not_under_gdb() {
    // Triggering a breakpoint involves raising SIGTRAP, which terminates
    // the process under normal circumstances
    breakpoint();
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
