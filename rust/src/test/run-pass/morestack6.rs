// This test attempts to force the dynamic linker to resolve
// external symbols as close to the red zone as possible.

extern mod rustrt {
    fn debug_get_stk_seg() -> *u8;

    fn rust_get_sched_id() -> libc::intptr_t;
    fn last_os_error() -> ~str;
    fn rust_getcwd() -> ~str;
    fn get_task_id() -> libc::intptr_t;
    fn rust_sched_threads();
    fn rust_get_task();
}

fn calllink01() { rustrt::rust_get_sched_id(); }
fn calllink02() { rustrt::last_os_error(); }
fn calllink03() { rustrt::rust_getcwd(); }
fn calllink08() { rustrt::get_task_id(); }
fn calllink09() { rustrt::rust_sched_threads(); }
fn calllink10() { rustrt::rust_get_task(); }

fn runtest(f: fn~(), frame_backoff: u32) {
    runtest2(f, frame_backoff, 0 as *u8);
}

fn runtest2(f: fn~(), frame_backoff: u32, last_stk: *u8) -> u32 {
    let curr_stk = rustrt::debug_get_stk_seg();
    if (last_stk != curr_stk && last_stk != 0 as *u8) {
        // We switched stacks, go back and try to hit the dynamic linker
        frame_backoff
    } else {
        let frame_backoff = runtest2(f, frame_backoff, curr_stk);
        if frame_backoff > 1u32 {
            frame_backoff - 1u32
        } else if frame_backoff == 1u32 {
            f();
            0u32
        } else {
            0u32
        }
    }
}

fn main() {
    let fns = ~[
        calllink01,
        calllink02,
        calllink03,
        calllink08,
        calllink09,
        calllink10
    ];
    let rng = rand::Rng();
    for fns.each |f| {
        let f = *f;
        let sz = rng.next() % 256u32 + 256u32;
        let frame_backoff = rng.next() % 10u32 + 1u32;
        task::try(|move f| runtest(f, frame_backoff) );
    }
}
