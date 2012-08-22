// Dynamic arenas.

// Arenas are used to quickly allocate objects that share a
// lifetime. The arena uses ~[u8] vectors as a backing store to
// allocate objects from. For each allocated object, the arena stores
// a pointer to the type descriptor followed by the
// object. (Potentially with alignment padding after each of them.)
// When the arena is destroyed, it iterates through all of its chunks,
// and uses the tydesc information to trace through the objects,
// calling the destructors on them.
// One subtle point that needs to be addressed is how to handle
// failures while running the user provided initializer function. It
// is important to not run the destructor on uninitalized objects, but
// how to detect them is somewhat subtle. Since alloc() can be invoked
// recursively, it is not sufficient to simply exclude the most recent
// object. To solve this without requiring extra space, we use the low
// order bit of the tydesc pointer to encode whether the object it
// describes has been fully initialized.

// As an optimization, objects with destructors are stored in
// different chunks than objects without destructors. This reduces
// overhead when initializing plain-old-data and means we don't need
// to waste time running the destructors of POD.

export arena, arena_with_size;

import list;
import list::{list, cons, nil};
import unsafe::reinterpret_cast;
import sys::TypeDesc;
import libc::size_t;

#[abi = "rust-intrinsic"]
extern mod rusti {
    fn move_val_init<T>(&dst: T, -src: T);
    fn needs_drop<T>() -> bool;
}
extern mod rustrt {
    #[rust_stack]
    fn rust_call_tydesc_glue(root: *u8, tydesc: *TypeDesc, field: size_t);
}
// This probably belongs somewhere else. Needs to be kept in sync with
// changes to glue...
const tydesc_drop_glue_index: size_t = 3 as size_t;

// The way arena uses arrays is really deeply awful. The arrays are
// allocated, and have capacities reserved, but the fill for the array
// will always stay at 0.
type chunk = {data: ~[u8], mut fill: uint, is_pod: bool};

struct arena {
    // The head is seperated out from the list as a unbenchmarked
    // microoptimization, to avoid needing to case on the list to
    // access the head.
    priv mut head: @chunk;
    priv mut pod_head: @chunk;
    priv mut chunks: @list<@chunk>;
    drop {
        unsafe {
            destroy_chunk(self.head);
            for list::each(self.chunks) |chunk| {
                if !chunk.is_pod { destroy_chunk(chunk); }
            }
        }
    }
}

fn chunk(size: uint, is_pod: bool) -> @chunk {
    let mut v = ~[];
    vec::reserve(v, size);
    @{ data: v, mut fill: 0u, is_pod: is_pod }
}

fn arena_with_size(initial_size: uint) -> arena {
    return arena {mut head: chunk(initial_size, false),
                  mut pod_head: chunk(initial_size, true),
                  mut chunks: @nil};
}

fn arena() -> arena {
    arena_with_size(32u)
}

#[inline(always)]
fn round_up_to(base: uint, align: uint) -> uint {
    (base + (align - 1)) & !(align - 1)
}

// Walk down a chunk, running the destructors for any objects stored
// in it.
unsafe fn destroy_chunk(chunk: @chunk) {
    let mut idx = 0;
    let buf = vec::unsafe::to_ptr(chunk.data);
    let fill = chunk.fill;

    while idx < fill {
        let tydesc_data: *uint = reinterpret_cast(ptr::offset(buf, idx));
        let (tydesc, is_done) = un_bitpack_tydesc_ptr(*tydesc_data);
        let size = (*tydesc).size, align = (*tydesc).align;

        let after_tydesc = idx + sys::size_of::<*TypeDesc>();

        let start = round_up_to(after_tydesc, align);

        //debug!("freeing object: idx = %u, size = %u, align = %u, done = %b",
        //       start, size, align, is_done);
        if is_done {
            rustrt::rust_call_tydesc_glue(
                ptr::offset(buf, start), tydesc, tydesc_drop_glue_index);
        }

        // Find where the next tydesc lives
        idx = round_up_to(start + size, sys::pref_align_of::<*TypeDesc>());
    }
}

// We encode whether the object a tydesc describes has been
// initialized in the arena in the low bit of the tydesc pointer. This
// is necessary in order to properly do cleanup if a failure occurs
// during an initializer.
#[inline(always)]
unsafe fn bitpack_tydesc_ptr(p: *TypeDesc, is_done: bool) -> uint {
    let p_bits: uint = reinterpret_cast(p);
    p_bits | (is_done as uint)
}
#[inline(always)]
unsafe fn un_bitpack_tydesc_ptr(p: uint) -> (*TypeDesc, bool) {
    (reinterpret_cast(p & !1), p & 1 == 1)
}

// The duplication between the POD and non-POD functions is annoying.
impl &arena {
    // Functions for the POD part of the arena
    fn alloc_pod_grow(n_bytes: uint, align: uint) -> *u8 {
        // Allocate a new chunk.
        let chunk_size = vec::capacity(self.pod_head.data);
        let new_min_chunk_size = uint::max(n_bytes, chunk_size);
        self.chunks = @cons(self.pod_head, self.chunks);
        self.pod_head =
            chunk(uint::next_power_of_two(new_min_chunk_size + 1u), true);

        return self.alloc_pod_inner(n_bytes, align);
    }

    #[inline(always)]
    fn alloc_pod_inner(n_bytes: uint, align: uint) -> *u8 {
        let head = self.pod_head;

        let start = round_up_to(head.fill, align);
        let end = start + n_bytes;
        if end > vec::capacity(head.data) {
            return self.alloc_pod_grow(n_bytes, align);
        }
        head.fill = end;

        //debug!("idx = %u, size = %u, align = %u, fill = %u",
        //       start, n_bytes, align, head.fill);

        unsafe {
            ptr::offset(vec::unsafe::to_ptr(head.data), start)
        }
    }

    #[inline(always)]
    fn alloc_pod<T>(op: fn() -> T) -> &self/T {
        unsafe {
            let tydesc = sys::get_type_desc::<T>();
            let ptr = self.alloc_pod_inner((*tydesc).size, (*tydesc).align);
            let ptr: *mut T = reinterpret_cast(ptr);
            rusti::move_val_init(*ptr, op());
            return reinterpret_cast(ptr);
        }
    }

    // Functions for the non-POD part of the arena
    fn alloc_nonpod_grow(n_bytes: uint, align: uint) -> (*u8, *u8) {
        // Allocate a new chunk.
        let chunk_size = vec::capacity(self.head.data);
        let new_min_chunk_size = uint::max(n_bytes, chunk_size);
        self.chunks = @cons(self.head, self.chunks);
        self.head =
            chunk(uint::next_power_of_two(new_min_chunk_size + 1u), false);

        return self.alloc_nonpod_inner(n_bytes, align);
    }

    #[inline(always)]
    fn alloc_nonpod_inner(n_bytes: uint, align: uint) -> (*u8, *u8) {
        let head = self.head;

        let tydesc_start = head.fill;
        let after_tydesc = head.fill + sys::size_of::<*TypeDesc>();
        let start = round_up_to(after_tydesc, align);
        let end = start + n_bytes;
        if end > vec::capacity(head.data) {
            return self.alloc_nonpod_grow(n_bytes, align);
        }
        head.fill = round_up_to(end, sys::pref_align_of::<*TypeDesc>());

        //debug!("idx = %u, size = %u, align = %u, fill = %u",
        //       start, n_bytes, align, head.fill);

        unsafe {
            let buf = vec::unsafe::to_ptr(head.data);
            return (ptr::offset(buf, tydesc_start), ptr::offset(buf, start));
        }
    }

    #[inline(always)]
    fn alloc_nonpod<T>(op: fn() -> T) -> &self/T {
        unsafe {
            let tydesc = sys::get_type_desc::<T>();
            let (ty_ptr, ptr) =
                self.alloc_nonpod_inner((*tydesc).size, (*tydesc).align);
            let ty_ptr: *mut uint = reinterpret_cast(ty_ptr);
            let ptr: *mut T = reinterpret_cast(ptr);
            // Write in our tydesc along with a bit indicating that it
            // has *not* been initialized yet.
            *ty_ptr = reinterpret_cast(tydesc);
            // Actually initialize it
            rusti::move_val_init(*ptr, op());
            // Now that we are done, update the tydesc to indicate that
            // the object is there.
            *ty_ptr = bitpack_tydesc_ptr(tydesc, true);

            return reinterpret_cast(ptr);
        }
    }

    // The external interface
    #[inline(always)]
    fn alloc<T>(op: fn() -> T) -> &self/T {
        if !rusti::needs_drop::<T>() {
            self.alloc_pod(op)
        } else { self.alloc_nonpod(op) }
    }
}

#[test]
fn test_arena_destructors() {
    let arena = arena::arena();
    for uint::range(0, 10) |i| {
        // Arena allocate something with drop glue to make sure it
        // doesn't leak.
        do arena.alloc { @i };
        // Allocate something with funny size and alignment, to keep
        // things interesting.
        do arena.alloc { [0u8, 1u8, 2u8]/3 };
    }
}

#[test]
#[should_fail]
fn test_arena_destructors_fail() {
    let arena = arena::arena();
    // Put some stuff in the arena.
    for uint::range(0, 10) |i| {
        // Arena allocate something with drop glue to make sure it
        // doesn't leak.
        do arena.alloc { @i };
        // Allocate something with funny size and alignment, to keep
        // things interesting.
        do arena.alloc { [0u8, 1u8, 2u8]/3 };
    }
    // Now, fail while allocating
    do arena.alloc::<@int> {
        // First, recursively allocate something else; that needs to
        // get freed too.
        do arena.alloc { @20 };
        // Now fail.
        fail;
    };
}
