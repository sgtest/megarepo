#ifndef RUST_INTERNAL_H
#define RUST_INTERNAL_H

#include "rust_globals.h"
#include "util/array_list.h"
#include "util/indexed_list.h"
#include "util/synchronized_indexed_list.h"
#include "util/hash_map.h"
#include "sync/sync.h"
#include "sync/lock_and_signal.h"
#include "sync/lock_free_queue.h"

struct rust_sched_loop;
struct rust_task;
class rust_log;
class rust_port;
class rust_kernel;

struct stk_seg;
struct type_desc;
struct frame_glue_fns;

typedef intptr_t rust_sched_id;
typedef intptr_t rust_task_id;
typedef intptr_t rust_port_id;

#define I(dom, e) ((e) ? (void)0 : \
         (dom)->srv->fatal(#e, __FILE__, __LINE__, ""))

#define W(dom, e, s, ...) ((e) ? (void)0 : \
         (dom)->srv->warning(#e, __FILE__, __LINE__, s, ## __VA_ARGS__))

#define A(dom, e, s, ...) ((e) ? (void)0 : \
         (dom)->srv->fatal(#e, __FILE__, __LINE__, s, ## __VA_ARGS__))

#define K(srv, e, s, ...) ((e) ? (void)0 : \
         srv->fatal(#e, __FILE__, __LINE__, s, ## __VA_ARGS__))

#define PTR "0x%" PRIxPTR

// This drives our preemption scheme.

static size_t const TIME_SLICE_IN_MS = 10;

// This accounts for logging buffers.

static size_t const BUF_BYTES = 2048;

// The error status to use when the process fails
#define PROC_FAIL_CODE 101

// Every reference counted object should use this macro and initialize
// ref_count.

#define RUST_REFCOUNTED(T) \
  RUST_REFCOUNTED_WITH_DTOR(T, delete (T*)this)
#define RUST_REFCOUNTED_WITH_DTOR(T, dtor) \
  intptr_t ref_count;      \
  void ref() { ++ref_count; } \
  void deref() { if (--ref_count == 0) { dtor; } }

#define RUST_ATOMIC_REFCOUNT()                                             \
private:                                                                   \
   intptr_t ref_count;                                                     \
public:                                                                    \
   void ref() {                                                            \
       intptr_t old = sync::increment(ref_count);                          \
       assert(old > 0);                                                    \
   }                                                                       \
   void deref() { if(0 == sync::decrement(ref_count)) { delete_this(); } } \
   intptr_t get_ref_count() { return sync::read(ref_count); }

template <typename T> struct task_owned {
    inline void *operator new(size_t size, rust_task *task, const char *tag);

    inline void *operator new[](size_t size, rust_task *task,
                                const char *tag);

    inline void *operator new(size_t size, rust_task &task, const char *tag);

    inline void *operator new[](size_t size, rust_task &task,
                                const char *tag);

    void operator delete(void *ptr) {
        ((T *)ptr)->task->free(ptr);
    }
};

template <typename T> struct kernel_owned {
    inline void *operator new(size_t size, rust_kernel *kernel,
                              const char *tag);

    void operator delete(void *ptr) {
        ((T *)ptr)->kernel->free(ptr);
    }
};

template <typename T> struct region_owned {
    void operator delete(void *ptr) {
        ((T *)ptr)->region->free(ptr);
    }
};

// A cond(ition) is something we can block on. This can be a channel
// (writing), a port (reading) or a task (waiting).

struct rust_cond { };

#include "memory_region.h"
#include "rust_srv.h"
#include "rust_log.h"
#include "rust_kernel.h"
#include "rust_sched_loop.h"

typedef void CDECL (glue_fn)(void *, void *,
                             const type_desc **, void *);

struct rust_shape_tables {
    uint8_t *tags;
    uint8_t *resources;
};

typedef unsigned long ref_cnt_t;

// Corresponds to the boxed data in the @ region.  The body follows the
// header; you can obtain a ptr via box_body() below.
struct rust_opaque_box {
    ref_cnt_t ref_count;
    type_desc *td;
    rust_opaque_box *prev;
    rust_opaque_box *next;
};

// The type of functions that we spawn, which fall into two categories:
// - the main function: has a NULL environment, but uses the void* arg
// - unique closures of type fn~(): have a non-NULL environment, but
//   no arguments (and hence the final void*) is harmless
typedef void (*CDECL spawn_fn)(void*, rust_opaque_box*, void *);

// corresponds to the layout of a fn(), fn@(), fn~() etc
struct fn_env_pair {
    spawn_fn f;
    rust_opaque_box *env;
};

static inline void *box_body(rust_opaque_box *box) {
    // Here we take advantage of the fact that the size of a box in 32
    // (resp. 64) bit is 16 (resp. 32) bytes, and thus always 16-byte aligned.
    // If this were to change, we would have to update the method
    // rustc::middle::trans::base::opaque_box_body() as well.
    return (void*)(box + 1);
}

struct type_desc {
    // First part of type_desc is known to compiler.
    // first_param = &descs[1] if dynamic, null if static.
    const type_desc **first_param;
    size_t size;
    size_t align;
    glue_fn *take_glue;
    glue_fn *drop_glue;
    glue_fn *free_glue;
    void *UNUSED;
    glue_fn *sever_glue;    // For GC.
    glue_fn *mark_glue;     // For GC.
    uintptr_t unused2;
    void *UNUSED_2;
    const uint8_t *shape;
    const rust_shape_tables *shape_tables;
    uintptr_t n_params;
    uintptr_t n_obj_params;

    // Residual fields past here are known only to runtime.
    UT_hash_handle hh;
    size_t n_descs;
    const type_desc *descs[];
};

extern "C" type_desc *rust_clone_type_desc(type_desc*);

#include "circular_buffer.h"
#include "rust_task.h"
#include "rust_port.h"
#include "memory.h"

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//

#endif
