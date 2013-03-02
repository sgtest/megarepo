// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[doc(hidden)];

use libc::{c_char, c_void, intptr_t, uintptr_t};
use ptr::{mut_null, null, to_unsafe_ptr};
use repr::BoxRepr;
use sys::TypeDesc;
use cast::transmute;

/**
 * Runtime structures
 *
 * NB: These must match the representation in the C++ runtime.
 */

type DropGlue = fn(**TypeDesc, *c_void);
type FreeGlue = fn(**TypeDesc, *c_void);

type TaskID = uintptr_t;

struct StackSegment { priv opaque: () }
struct Scheduler { priv opaque: () }
struct SchedulerLoop { priv opaque: () }
struct Kernel { priv opaque: () }
struct Env { priv opaque: () }
struct AllocHeader { priv opaque: () }
struct MemoryRegion { priv opaque: () }

#[cfg(target_arch="x86")]
#[cfg(target_arch="arm")]
struct Registers {
    data: [u32 * 16]
}

#[cfg(target_arch="x86")]
#[cfg(target_arch="arm")]
struct Context {
    regs: Registers,
    next: *Context,
    pad: [u32 * 3]
}

#[cfg(target_arch="x86_64")]
struct Registers {
    data: [u64 * 22]
}

#[cfg(target_arch="x86_64")]
struct Context {
    regs: Registers,
    next: *Context,
    pad: uintptr_t
}

struct BoxedRegion {
    env: *Env,
    backing_region: *MemoryRegion,
    live_allocs: *BoxRepr
}

#[cfg(target_arch="x86")]
#[cfg(target_arch="arm")]
struct Task {
    // Public fields
    refcount: intptr_t,                 // 0
    id: TaskID,                         // 4
    pad: [u32 * 2],                     // 8
    ctx: Context,                       // 16
    stack_segment: *StackSegment,       // 96
    runtime_sp: uintptr_t,              // 100
    scheduler: *Scheduler,              // 104
    scheduler_loop: *SchedulerLoop,     // 108

    // Fields known only to the runtime
    kernel: *Kernel,                    // 112
    name: *c_char,                      // 116
    list_index: i32,                    // 120
    boxed_region: BoxedRegion           // 128
}

#[cfg(target_arch="x86_64")]
struct Task {
    // Public fields
    refcount: intptr_t,
    id: TaskID,
    ctx: Context,
    stack_segment: *StackSegment,
    runtime_sp: uintptr_t,
    scheduler: *Scheduler,
    scheduler_loop: *SchedulerLoop,

    // Fields known only to the runtime
    kernel: *Kernel,
    name: *c_char,
    list_index: i32,
    boxed_region: BoxedRegion
}

/*
 * Box annihilation
 *
 * This runs at task death to free all boxes.
 */

struct AnnihilateStats {
    n_total_boxes: uint,
    n_unique_boxes: uint,
    n_bytes_freed: uint
}

unsafe fn each_live_alloc(f: fn(box: *mut BoxRepr, uniq: bool) -> bool) {
    use managed;

    let task: *Task = transmute(rustrt::rust_get_task());
    let box = (*task).boxed_region.live_allocs;
    let mut box: *mut BoxRepr = transmute(copy box);
    while box != mut_null() {
        let next = transmute(copy (*box).header.next);
        let uniq =
            (*box).header.ref_count == managed::raw::RC_MANAGED_UNIQUE;

        if ! f(box, uniq) {
            break
        }

        box = next
    }
}

#[cfg(unix)]
fn debug_mem() -> bool {
    use os;
    use libc;
    do os::as_c_charp("RUST_DEBUG_MEM") |p| {
        unsafe { libc::getenv(p) != null() }
    }
}

#[cfg(windows)]
fn debug_mem() -> bool {
    false
}

/// Destroys all managed memory (i.e. @ boxes) held by the current task.
#[cfg(notest)]
#[lang="annihilate"]
pub unsafe fn annihilate() {
    use unstable::lang::local_free;
    use io::WriterUtil;
    use io;
    use libc;
    use sys;
    use managed;

    let mut stats = AnnihilateStats {
        n_total_boxes: 0,
        n_unique_boxes: 0,
        n_bytes_freed: 0
    };

    // Pass 1: Make all boxes immortal.
    for each_live_alloc |box, uniq| {
        stats.n_total_boxes += 1;
        if uniq {
            stats.n_unique_boxes += 1;
        } else {
            (*box).header.ref_count = managed::raw::RC_IMMORTAL;
        }
    }

    // Pass 2: Drop all boxes.
    for each_live_alloc |box, uniq| {
        if !uniq {
            let tydesc: *TypeDesc = transmute(copy (*box).header.type_desc);
            let drop_glue: DropGlue = transmute(((*tydesc).drop_glue, 0));
            drop_glue(to_unsafe_ptr(&tydesc), transmute(&(*box).data));
        }
    }

    // Pass 3: Free all boxes.
    for each_live_alloc |box, uniq| {
        if !uniq {
            stats.n_bytes_freed +=
                (*((*box).header.type_desc)).size
                + sys::size_of::<BoxRepr>();
            local_free(transmute(box));
        }
    }

    if debug_mem() {
        // We do logging here w/o allocation.
        let dbg = libc::STDERR_FILENO as io::fd_t;
        dbg.write_str("annihilator stats:");
        dbg.write_str("\n  total_boxes: ");
        dbg.write_uint(stats.n_total_boxes);
        dbg.write_str("\n  unique_boxes: ");
        dbg.write_uint(stats.n_unique_boxes);
        dbg.write_str("\n  bytes_freed: ");
        dbg.write_uint(stats.n_bytes_freed);
        dbg.write_str("\n");
    }
}

/// Bindings to the runtime
extern mod rustrt {
    #[rust_stack]
    // FIXME (#4386): Unable to make following method private.
    pub unsafe fn rust_get_task() -> *c_void;
}

