#include "rust_cc.h"
#include "rust_gc.h"
#include "rust_internal.h"
#include "rust_scheduler.h"
#include "rust_unwind.h"
#include "rust_upcall.h"
#include <stdint.h>

// Upcalls.

#if defined(__i386__) || defined(__x86_64__) || defined(_M_X64)
void
check_stack(rust_task *task) {
    void *esp;
#   ifdef __i386__
    asm volatile("movl %%esp,%0" : "=r" (esp));
#   else
    asm volatile("mov %%rsp,%0" : "=r" (esp));
#   endif
    if (esp < task->stk->data)
        task->kernel->fatal("Out of stack space, sorry");
}
#else
#warning "Stack checks are not supported on this architecture"
void
check_stack(rust_task *task) {
    // TODO
}
#endif

// Copy elements from one vector to another,
// dealing with reference counts
static inline void
copy_elements(rust_task *task, type_desc *elem_t,
              void *pdst, void *psrc, size_t n) {
    char *dst = (char *)pdst, *src = (char *)psrc;
    memmove(dst, src, n);

    // increment the refcount of each element of the vector
    if (elem_t->take_glue) {
        glue_fn *take_glue = elem_t->take_glue;
        size_t elem_size = elem_t->size;
        const type_desc **tydescs = elem_t->first_param;
        for (char *p = dst; p < dst+n; p += elem_size) {
            take_glue(NULL, NULL, tydescs, p);
        }
    }
}

extern "C" CDECL void
upcall_fail(char const *expr,
            char const *file,
            size_t line) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);
    LOG_ERR(task, upcall, "upcall fail '%s', %s:%" PRIdPTR, expr, file, line);
    task->fail();
}

extern "C" CDECL uintptr_t
upcall_malloc(size_t nbytes, type_desc *td) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);

    LOG(task, mem,
        "upcall malloc(%" PRIdPTR ", 0x%" PRIxPTR ")",
        nbytes, td);

    gc::maybe_gc(task);
    cc::maybe_cc(task);

    // TODO: Maybe use dladdr here to find a more useful name for the
    // type_desc.

    void *p = task->malloc(nbytes, "tdesc", td);
    memset(p, '\0', nbytes);

    task->local_allocs[p] = td;
    debug::maybe_track_origin(task, p);

    LOG(task, mem,
        "upcall malloc(%" PRIdPTR ", 0x%" PRIxPTR ") = 0x%" PRIxPTR,
        nbytes, td, (uintptr_t)p);
    return (uintptr_t) p;
}

/**
 * Called whenever an object's ref count drops to zero.
 */
extern "C" CDECL void
upcall_free(void* ptr, uintptr_t is_gc) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);

    rust_scheduler *sched = task->sched;
    DLOG(sched, mem,
             "upcall free(0x%" PRIxPTR ", is_gc=%" PRIdPTR ")",
             (uintptr_t)ptr, is_gc);

    task->local_allocs.erase(ptr);
    debug::maybe_untrack_origin(task, ptr);

    task->free(ptr, (bool) is_gc);
}

extern "C" CDECL uintptr_t
upcall_shared_malloc(size_t nbytes, type_desc *td) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);

    LOG(task, mem,
                   "upcall shared_malloc(%" PRIdPTR ", 0x%" PRIxPTR ")",
                   nbytes, td);
    void *p = task->kernel->malloc(nbytes, "shared malloc");
    memset(p, '\0', nbytes);
    LOG(task, mem,
                   "upcall shared_malloc(%" PRIdPTR ", 0x%" PRIxPTR
                   ") = 0x%" PRIxPTR,
                   nbytes, td, (uintptr_t)p);
    return (uintptr_t) p;
}

/**
 * Called whenever an object's ref count drops to zero.
 */
extern "C" CDECL void
upcall_shared_free(void* ptr) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);

    rust_scheduler *sched = task->sched;
    DLOG(sched, mem,
             "upcall shared_free(0x%" PRIxPTR")",
             (uintptr_t)ptr);
    task->kernel->free(ptr);
}

extern "C" CDECL type_desc *
upcall_get_type_desc(void *curr_crate, // ignored, legacy compat.
                     size_t size,
                     size_t align,
                     size_t n_descs,
                     type_desc const **descs,
                     uintptr_t n_obj_params) {
    rust_task *task = rust_scheduler::get_task();
    check_stack(task);
    LOG_UPCALL_ENTRY(task);

    LOG(task, cache, "upcall get_type_desc with size=%" PRIdPTR
        ", align=%" PRIdPTR ", %" PRIdPTR " descs", size, align,
        n_descs);
    rust_crate_cache *cache = task->get_crate_cache();
    type_desc *td = cache->get_type_desc(size, align, n_descs, descs,
                                         n_obj_params);
    LOG(task, cache, "returning tydesc 0x%" PRIxPTR, td);
    return td;
}

extern "C" CDECL void
upcall_vec_grow(rust_vec** vp, size_t new_sz) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);
    reserve_vec(task, vp, new_sz);
    (*vp)->fill = new_sz;
}

extern "C" CDECL void
upcall_vec_push(rust_vec** vp, type_desc* elt_ty, void* elt) {
    rust_task *task = rust_scheduler::get_task();
    LOG_UPCALL_ENTRY(task);
    size_t new_sz = (*vp)->fill + elt_ty->size;
    reserve_vec(task, vp, new_sz);
    rust_vec* v = *vp;
    copy_elements(task, elt_ty, &v->data[0] + v->fill, elt, elt_ty->size);
    v->fill += elt_ty->size;
}

/**
 * Returns a token that can be used to deallocate all of the allocated space
 * space in the dynamic stack.
 */
extern "C" CDECL void *
upcall_dynastack_mark() {
    return rust_scheduler::get_task()->dynastack.mark();
}

/**
 * Allocates space in the dynamic stack and returns it.
 *
 * FIXME: Deprecated since dynamic stacks need to be self-describing for GC.
 */
extern "C" CDECL void *
upcall_dynastack_alloc(size_t sz) {
    return sz ? rust_scheduler::get_task()->dynastack.alloc(sz, NULL) : NULL;
}

/**
 * Allocates space associated with a type descriptor in the dynamic stack and
 * returns it.
 */
extern "C" CDECL void *
upcall_dynastack_alloc_2(size_t sz, type_desc *ty) {
    return sz ? rust_scheduler::get_task()->dynastack.alloc(sz, ty) : NULL;
}

/** Frees space in the dynamic stack. */
extern "C" CDECL void
upcall_dynastack_free(void *ptr) {
    return rust_scheduler::get_task()->dynastack.free(ptr);
}

/**
 * Allocates |nbytes| bytes in the C stack and returns a pointer to the start
 * of the allocated space.
 */
extern "C" CDECL void *
upcall_alloc_c_stack(size_t nbytes) {
    rust_scheduler *sched = rust_scheduler::get_task()->sched;
    return sched->c_context.alloc_stack(nbytes);
}

extern "C" _Unwind_Reason_Code
__gxx_personality_v0(int version,
                     _Unwind_Action actions,
                     uint64_t exception_class,
                     _Unwind_Exception *ue_header,
                     _Unwind_Context *context);

extern "C" _Unwind_Reason_Code
upcall_rust_personality(int version,
                        _Unwind_Action actions,
                        uint64_t exception_class,
                        _Unwind_Exception *ue_header,
                        _Unwind_Context *context) {
    return __gxx_personality_v0(version,
                                actions,
                                exception_class,
                                ue_header,
                                context);
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
