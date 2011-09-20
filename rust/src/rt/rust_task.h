/*
 *
 */

#ifndef RUST_TASK_H
#define RUST_TASK_H

#include <map>

#include "util/array_list.h"

#include "context.h"
#include "rust_obstack.h"

struct rust_box;

struct stk_seg {
    unsigned int valgrind_id;
    uintptr_t limit;
    uint8_t data[];
};

struct frame_glue_fns {
    uintptr_t mark_glue_off;
    uintptr_t drop_glue_off;
    uintptr_t reloc_glue_off;
};

// portions of the task structure that are accessible from the standard
// library. This struct must agree with the std::task::rust_task record.
struct rust_task_user {
    rust_task_id id;
    uint32_t notify_enabled;   // this is way more bits than necessary, but it
                               // simplifies the alignment.
    chan_handle notify_chan;
    uintptr_t rust_sp;         // Saved sp when not running.
};

// std::lib::task::task_result
enum task_result {
    tr_success = 0,
    tr_failure = 1
};

// std::lib::task::task_notification
//
// since it's currently a unary tag, we only add the fields.
struct task_notification {
    rust_task_id id;
    task_result result; // task_result
};

struct
rust_task : public kernel_owned<rust_task>, rust_cond
{
    rust_task_user user;

    RUST_ATOMIC_REFCOUNT();

    // Fields known to the compiler.
    context ctx;
    stk_seg *stk;
    uintptr_t runtime_sp;      // Runtime sp while task running.
    rust_scheduler *sched;
    rust_crate_cache *cache;

    // Fields known only to the runtime.
    rust_kernel *kernel;
    const char *const name;
    rust_task_list *state;
    rust_cond *cond;
    const char *cond_name;
    rust_task *supervisor;     // Parent-link for failure propagation.
    int32_t list_index;

    rust_port_id next_port_id;

    // Keeps track of the last time this task yielded.
    timer yield_timer;

    // Rendezvous pointer for receiving data when blocked on a port. If we're
    // trying to read data and no data is available on any incoming channel,
    // we block on the port, and yield control to the scheduler. Since, we
    // were not able to read anything, we remember the location where the
    // result should go in the rendezvous_ptr, and let the sender write to
    // that location before waking us up.
    uintptr_t* rendezvous_ptr;

    // List of tasks waiting for this task to finish.
    array_list<rust_task *> tasks_waiting_to_join;

    // This flag indicates that a worker is either currently running the task
    // or is about to run this task.
    int running_on;
    int pinned_on;

    memory_region local_region;

    class wakeup_callback {
    public:
        virtual void on_wakeup() = 0;
    };

    wakeup_callback *_on_wakeup;

    // Indicates that the task ended in failure
    bool failed;
    // Indicates that the task was killed and needs to unwind
    bool killed;
    bool propagate_failure;

    lock_and_signal lock;

    hash_map<rust_port_id, rust_port *> port_table;

    rust_obstack dynastack;

    std::map<void *,type_desc *> local_allocs;

    // Only a pointer to 'name' is kept, so it must live as long as this task.
    rust_task(rust_scheduler *sched,
              rust_task_list *state,
              rust_task *spawner,
              const char *name);

    ~rust_task();

    void start(uintptr_t spawnee_fn,
               uintptr_t args,
               uintptr_t env);
    void start(uintptr_t spawnee_fn,
               uintptr_t args);
    void start();
    void grow(size_t n_frame_bytes);
    bool running();
    bool blocked();
    bool blocked_on(rust_cond *cond);
    bool dead();

    void *malloc(size_t sz, const char *tag, type_desc *td=0);
    void *realloc(void *data, size_t sz, bool gc_mem=false);
    void free(void *p, bool gc_mem=false);

    void transition(rust_task_list *src, rust_task_list *dst);

    void block(rust_cond *on, const char* name);
    void wakeup(rust_cond *from);
    void die();
    void unblock();

    // Print a backtrace, if the "bt" logging option is on.
    void backtrace();

    // Save callee-saved registers and return to the main loop.
    void yield();

    // Yields for a specified duration of time.
    void yield(size_t time_in_ms);

    // Fail this task (assuming caller-on-stack is different task).
    void kill();

    // Fail self, assuming caller-on-stack is this task.
    void fail();
    void conclude_failure();
    void fail_parent();

    // Disconnect from our supervisor.
    void unsupervise();

    // Notify tasks waiting for us that we are about to die.
    void notify_tasks_waiting_to_join();

    frame_glue_fns *get_frame_glue_fns(uintptr_t fp);
    rust_crate_cache * get_crate_cache();

    bool can_schedule(int worker);

    void *calloc(size_t size, const char *tag);

    void pin();
    void pin(int id);
    void unpin();

    void on_wakeup(wakeup_callback *callback);

    rust_port_id register_port(rust_port *port);
    void release_port(rust_port_id id);
    rust_port *get_port_by_id(rust_port_id id);

    // Use this function sparingly. Depending on the ref count is generally
    // not at all safe.
    intptr_t get_ref_count() const { return ref_count; }

    rust_chan *get_chan_by_handle(chan_handle *handle);
};

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

#endif /* RUST_TASK_H */
