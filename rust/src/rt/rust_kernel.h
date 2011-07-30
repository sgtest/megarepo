// -*- c++ -*-
#ifndef RUST_KERNEL_H
#define RUST_KERNEL_H

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

    const size_t num_threads;

    array_list<rust_scheduler *> threads;

    randctx rctx;

    rust_scheduler *create_scheduler(int id);
    void destroy_scheduler(rust_scheduler *sched);

    void create_schedulers();
    void destroy_schedulers();

public:

    int rval;

    volatile int live_tasks;

    struct rust_env *env;

    rust_kernel(rust_srv *srv, size_t num_threads);

    bool is_deadlocked();

    void signal_kernel_lock();
    void wakeup_schedulers();

    void log_all_scheduler_state();
    void log(uint32_t level, char const *fmt, ...);
    void fatal(char const *fmt, ...);
    virtual ~rust_kernel();

    void *malloc(size_t size, const char *tag);
    void *realloc(void *mem, size_t size);
    void free(void *mem);

    int start_task_threads();

#ifdef __WIN32__
    void win32_require(LPCTSTR fn, BOOL ok);
#endif

    rust_task *create_task(rust_task *spawner, const char *name);
};

#endif /* RUST_KERNEL_H */
