#ifndef RUST_TASK_THREAD_H
#define RUST_TASK_THREAD_H

#include "sync/rust_thread.h"
#include "rust_stack.h"
#include "context.h"

#ifndef _WIN32
#include <pthread.h>
#else
#include <windows.h>
#endif

struct rust_task_thread;

struct rust_hashable_dict {
    UT_hash_handle hh;
    void* fields[0];
};

class rust_crate_cache {
public:
    type_desc *get_type_desc(size_t size,
                             size_t align,
                             size_t n_descs,
                             type_desc const **descs,
                             uintptr_t n_obj_params);
    void** get_dict(size_t n_fields, void** dict);

private:

    type_desc *type_descs;
    rust_hashable_dict *dicts;

public:

    rust_task_thread *thread;
    size_t idx;

    rust_crate_cache(rust_task_thread *thread);
    ~rust_crate_cache();
    void flush();
};

struct rust_task_thread : public kernel_owned<rust_task_thread>,
                        rust_thread
{
    RUST_REFCOUNTED(rust_task_thread)

    // Fields known only by the runtime:
    rust_log _log;

    // NB: this is used to filter *runtime-originating* debug
    // logging, on a per-scheduler basis. It's not likely what
    // you want to expose to the user in terms of per-task
    // or per-module logging control. By default all schedulers
    // are set to debug-level logging here, and filtered by
    // runtime category using the pseudo-modules ::rt::foo.
    uint32_t log_lvl;

    rust_srv *srv;
    const char *const name;

    rust_task_list newborn_tasks;
    rust_task_list running_tasks;
    rust_task_list blocked_tasks;
    rust_task_list dead_tasks;

    rust_crate_cache cache;

    randctx rctx;

    rust_kernel *kernel;
    rust_scheduler *sched;
    int32_t list_index;

    const int id;

    lock_and_signal lock;
    size_t min_stack_size;

#ifndef __WIN32__
    pthread_attr_t attr;
    static pthread_key_t task_key;
#else
    static DWORD task_key;
#endif

    static bool tls_initialized;

    rust_env *env;
    context c_context;

    bool should_exit;

private:

    stk_seg *cached_c_stack;
    stk_seg *extra_c_stack;

    void prepare_c_stack();
    void unprepare_c_stack();

public:

    // Only a pointer to 'name' is kept, so it must live as long as this
    // domain.
    rust_task_thread(rust_scheduler *sched, rust_srv *srv, int id);
    ~rust_task_thread();
    void activate(rust_task *task);
    void log(rust_task *task, uint32_t level, char const *fmt, ...);
    rust_log & get_log();
    void fail();

    rust_crate_cache *get_cache();
    size_t number_of_live_tasks();

    void reap_dead_tasks();
    rust_task *schedule_task();

    void start_main_loop();

    void log_state();

    void kill_all_tasks();

    rust_task_id create_task(rust_task *spawner, const char *name,
                             size_t init_stack_sz);

    virtual void run();

    void init_tls();
    void place_task_in_tls(rust_task *task);

    static rust_task *get_task();

    // Called by each task when they are ready to be destroyed
    void release_task(rust_task *task);

    // Tells the scheduler to exit it's scheduling loop and thread
    void exit();

    // Called by tasks when they need a stack on which to run C code
    stk_seg *borrow_c_stack();
    void return_c_stack(stk_seg *stack);
};

inline rust_log &
rust_task_thread::get_log() {
    return _log;
}

#ifndef __WIN32__

inline rust_task *
rust_task_thread::get_task() {
    if (!tls_initialized)
        return NULL;
    rust_task *task = reinterpret_cast<rust_task *>
        (pthread_getspecific(task_key));
    assert(task && "Couldn't get the task from TLS!");
    return task;
}

#else

inline rust_task *
rust_task_thread::get_task() {
    if (!tls_initialized)
        return NULL;
    rust_task *task = reinterpret_cast<rust_task *>(TlsGetValue(task_key));
    assert(task && "Couldn't get the task from TLS!");
    return task;
}

#endif

// NB: Runs on the Rust stack
inline stk_seg *
rust_task_thread::borrow_c_stack() {
    I(this, cached_c_stack);
    stk_seg *your_stack;
    if (extra_c_stack) {
        your_stack = extra_c_stack;
        extra_c_stack = NULL;
    } else {
        your_stack = cached_c_stack;
        cached_c_stack = NULL;
    }
    return your_stack;
}

// NB: Runs on the Rust stack
inline void
rust_task_thread::return_c_stack(stk_seg *stack) {
    I(this, !extra_c_stack);
    if (!cached_c_stack) {
        cached_c_stack = stack;
    } else {
        extra_c_stack = stack;
    }
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

#endif /* RUST_TASK_THREAD_H */
