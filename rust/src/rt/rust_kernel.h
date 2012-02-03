// -*- c++ -*-
#ifndef RUST_KERNEL_H
#define RUST_KERNEL_H

#include "memory_region.h"
#include "rust_log.h"

struct rust_scheduler;

/**
 * A global object shared by all thread domains. Most of the data structures
 * in this class are synchronized since they are accessed from multiple
 * threads.
 */
class rust_kernel {
    memory_region _region;
    rust_log _log;

public:
    rust_srv *srv;
private:
    lock_and_signal _kernel_lock;

    array_list<rust_scheduler *> threads;

    randctx rctx;

    rust_scheduler *create_scheduler(int id);
    void destroy_scheduler(rust_scheduler *sched);

    void create_schedulers();
    void destroy_schedulers();

    rust_task_id max_id;
    hash_map<rust_task_id, rust_task *> task_table;

    int rval;

public:
    const size_t num_threads;

    volatile int live_tasks;
    struct rust_env *env;

    rust_kernel(rust_srv *srv, size_t num_threads);

    bool is_deadlocked();

    void signal_kernel_lock();
    void exit_schedulers();

    void log_all_scheduler_state();
    void log(uint32_t level, char const *fmt, ...);
    void fatal(char const *fmt, ...);
    virtual ~rust_kernel();

    void *malloc(size_t size, const char *tag);
    void *realloc(void *mem, size_t size);
    void free(void *mem);

    void fail();

    int start_task_threads();

#ifdef __WIN32__
    void win32_require(LPCTSTR fn, BOOL ok);
#endif

    rust_task_id create_task(rust_task *spawner, const char *name,
			     size_t init_stack_size);
    rust_task_id create_task(rust_task * spawner, const char *name);
    rust_task *get_task_by_id(rust_task_id id);
    void release_task_id(rust_task_id tid);
    void set_exit_status(int code);
};

#endif /* RUST_KERNEL_H */
