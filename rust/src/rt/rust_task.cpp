
#include "rust_internal.h"
#include "rust_cc.h"

#include "vg/valgrind.h"
#include "vg/memcheck.h"

#ifndef __WIN32__
#include <execinfo.h>
#endif
#include <iostream>
#include <cassert>
#include <cstring>
#include <algorithm>

#include "globals.h"
#include "rust_upcall.h"

// The amount of extra space at the end of each stack segment, available
// to the rt, compiler and dynamic linker for running small functions
// FIXME: We want this to be 128 but need to slim the red zone calls down
#define RZ_LINUX_32 (1024*20)
#define RZ_LINUX_64 (1024*20)
#define RZ_MAC_32   (1024*20)
#define RZ_MAC_64   (1024*20)
#define RZ_WIN_32   (1024*20)
#define RZ_BSD_32   (1024*20)
#define RZ_BSD_64   (1024*20)

#ifdef __linux__
#ifdef __i386__
#define RED_ZONE_SIZE RZ_LINUX_32
#endif
#ifdef __x86_64__
#define RED_ZONE_SIZE RZ_LINUX_64
#endif
#endif
#ifdef __APPLE__
#ifdef __i386__
#define RED_ZONE_SIZE RZ_MAC_32
#endif
#ifdef __x86_64__
#define RED_ZONE_SIZE RZ_MAC_64
#endif
#endif
#ifdef __WIN32__
#ifdef __i386__
#define RED_ZONE_SIZE RZ_WIN_32
#endif
#ifdef __x86_64__
#define RED_ZONE_SIZE RZ_WIN_64
#endif
#endif
#ifdef __FreeBSD__
#ifdef __i386__
#define RED_ZONE_SIZE RZ_BSD_32
#endif
#ifdef __x86_64__
#define RED_ZONE_SIZE RZ_BSD_64
#endif
#endif

// A value that goes at the end of the stack and must not be touched
const uint8_t stack_canary[] = {0xAB, 0xCD, 0xAB, 0xCD,
                                0xAB, 0xCD, 0xAB, 0xCD,
                                0xAB, 0xCD, 0xAB, 0xCD,
                                0xAB, 0xCD, 0xAB, 0xCD};

static size_t
get_next_stk_size(rust_scheduler *sched, rust_task *task,
                  size_t min, size_t current, size_t requested) {
    LOG(task, mem, "calculating new stack size for 0x%" PRIxPTR, task);
    LOG(task, mem,
        "min: %" PRIdPTR " current: %" PRIdPTR " requested: %" PRIdPTR,
        min, current, requested);

    // Allocate at least enough to accomodate the next frame
    size_t sz = std::max(min, requested);

    // And double the stack size each allocation
    const size_t max = 1024 * 1024;
    size_t next = std::min(max, current * 2);

    sz = std::max(sz, next);

    LOG(task, mem, "next stack size: %" PRIdPTR, sz);
    I(sched, requested <= sz);
    return sz;
}

// Task stack segments. Heap allocated and chained together.

static void
config_valgrind_stack(stk_seg *stk) {
    stk->valgrind_id =
        VALGRIND_STACK_REGISTER(&stk->data[0],
                                stk->end);
#ifndef NVALGRIND
    // Establish that the stack is accessible.  This must be done when reusing
    // old stack segments, since the act of popping the stack previously
    // caused valgrind to consider the whole thing inaccessible.
    size_t sz = stk->end - (uintptr_t)&stk->data[0];
    VALGRIND_MAKE_MEM_UNDEFINED(stk->data + sizeof(stack_canary),
                                sz - sizeof(stack_canary));
#endif
}

static void
unconfig_valgrind_stack(stk_seg *stk) {
VALGRIND_STACK_DEREGISTER(stk->valgrind_id);
}

static void
add_stack_canary(stk_seg *stk) {
    memcpy(stk->data, stack_canary, sizeof(stack_canary));
    assert(sizeof(stack_canary) == 16 && "Stack canary was not the expected size");
}

static void
check_stack_canary(stk_seg *stk) {
    assert(!memcmp(stk->data, stack_canary, sizeof(stack_canary))
      && "Somebody killed the canary");
}

// The amount of stack in a segment available to Rust code
static size_t
user_stack_size(stk_seg *stk) {
    return (size_t)(stk->end
                    - (uintptr_t)&stk->data[0]
                    - RED_ZONE_SIZE);
}

static void
free_stk(rust_task *task, stk_seg *stk) {
    LOGPTR(task->sched, "freeing stk segment", (uintptr_t)stk);
    task->total_stack_sz -= user_stack_size(stk);
    task->free(stk);
}

static stk_seg*
new_stk(rust_scheduler *sched, rust_task *task, size_t requested_sz)
{
    LOG(task, mem, "creating new stack for task %" PRIxPTR, task);
    if (task->stk) {
        check_stack_canary(task->stk);
    }

    // The minimum stack size, in bytes, of a Rust stack, excluding red zone
    size_t min_sz = sched->min_stack_size;

    // Try to reuse an existing stack segment
    if (task->stk != NULL && task->stk->prev != NULL) {
        size_t prev_sz = user_stack_size(task->stk->prev);
        if (min_sz <= prev_sz && requested_sz <= prev_sz) {
            LOG(task, mem, "reusing existing stack");
            task->stk = task->stk->prev;
            A(sched, task->stk->prev == NULL, "Bogus stack ptr");
            config_valgrind_stack(task->stk);
            return task->stk;
        } else {
            LOG(task, mem, "existing stack is not big enough");
            free_stk(task, task->stk->prev);
            task->stk->prev = NULL;
        }
    }

    // The size of the current stack segment, excluding red zone
    size_t current_sz = 0;
    if (task->stk != NULL) {
        current_sz = user_stack_size(task->stk);
    }
    // The calculated size of the new stack, excluding red zone
    size_t rust_stk_sz = get_next_stk_size(sched, task, min_sz,
                                           current_sz, requested_sz);

    if (task->total_stack_sz + rust_stk_sz > sched->env->max_stack_size) {
        LOG_ERR(task, task, "task %" PRIxPTR " ran out of stack", task);
        task->fail();
    }

    size_t sz = sizeof(stk_seg) + rust_stk_sz + RED_ZONE_SIZE;
    stk_seg *stk = (stk_seg *)task->malloc(sz, "stack");
    LOGPTR(task->sched, "new stk", (uintptr_t)stk);
    memset(stk, 0, sizeof(stk_seg));
    add_stack_canary(stk);
    stk->prev = NULL;
    stk->next = task->stk;
    stk->end = (uintptr_t) &stk->data[rust_stk_sz + RED_ZONE_SIZE];
    LOGPTR(task->sched, "stk end", stk->end);

    task->stk = stk;
    config_valgrind_stack(task->stk);
    task->total_stack_sz += user_stack_size(stk);
    return stk;
}

static void
del_stk(rust_task *task, stk_seg *stk)
{
    assert(stk == task->stk && "Freeing stack segments out of order!");
    check_stack_canary(stk);

    task->stk = stk->next;

    bool delete_stack = false;
    if (task->stk != NULL) {
        // Don't actually delete this stack. Save it to reuse later,
        // preventing the pathological case where we repeatedly reallocate
        // the stack for the next frame.
        task->stk->prev = stk;
    } else {
        // This is the last stack, delete it.
        delete_stack = true;
    }

    // Delete the previous previous stack
    if (stk->prev != NULL) {
        free_stk(task, stk->prev);
        stk->prev = NULL;
    }

    unconfig_valgrind_stack(stk);
    if (delete_stack) {
        free_stk(task, stk);
        A(task->sched, task->total_stack_sz == 0, "Stack size should be 0");
    }
}

// Tasks
rust_task::rust_task(rust_scheduler *sched, rust_task_list *state,
                     rust_task *spawner, const char *name,
                     size_t init_stack_sz) :
    ref_count(1),
    stk(NULL),
    runtime_sp(0),
    sched(sched),
    cache(NULL),
    kernel(sched->kernel),
    name(name),
    state(state),
    cond(NULL),
    cond_name("none"),
    supervisor(spawner),
    list_index(-1),
    next_port_id(0),
    rendezvous_ptr(0),
    local_region(&sched->srv->local_region),
    boxed(&local_region),
    unwinding(false),
    killed(false),
    propagate_failure(true),
    dynastack(this),
    cc_counter(0),
    total_stack_sz(0)
{
    LOGPTR(sched, "new task", (uintptr_t)this);
    DLOG(sched, task, "sizeof(task) = %d (0x%x)", sizeof *this, sizeof *this);

    assert((void*)this == (void*)&user);

    user.notify_enabled = 0;

    stk = new_stk(sched, this, init_stack_sz);
    user.rust_sp = stk->end;
    if (supervisor) {
        supervisor->ref();
    }
}

rust_task::~rust_task()
{
    I(sched, !sched->lock.lock_held_by_current_thread());
    I(sched, port_table.is_empty());
    DLOG(sched, task, "~rust_task %s @0x%" PRIxPTR ", refcnt=%d",
         name, (uintptr_t)this, ref_count);

    if (supervisor) {
        supervisor->deref();
    }

    kernel->release_task_id(user.id);

    /* FIXME: tighten this up, there are some more
       assertions that hold at task-lifecycle events. */
    I(sched, ref_count == 0); // ||
    //   (ref_count == 1 && this == sched->root_task));

    // Delete all the stacks. There may be more than one if the task failed
    // and no landing pads stopped to clean up.
    while (stk != NULL) {
        del_stk(this, stk);
    }
}

struct spawn_args {
    rust_task *task;
    spawn_fn f;
    rust_opaque_box *envptr;
    void *argptr;
};

struct cleanup_args {
    spawn_args *spargs;
    bool threw_exception;
};

void
cleanup_task(cleanup_args *args) {
    spawn_args *a = args->spargs;
    bool threw_exception = args->threw_exception;
    rust_task *task = a->task;

    cc::do_cc(task);

    task->die();

    if (task->killed && !threw_exception) {
        LOG(task, task, "Task killed during termination");
        threw_exception = true;
    }

    task->notify(!threw_exception);

    if (threw_exception) {
#ifndef __WIN32__
        task->conclude_failure();
#else
        A(task->sched, false, "Shouldn't happen");
#endif
    }
}

// This runs on the Rust stack
extern "C" CDECL
void task_start_wrapper(spawn_args *a)
{
    rust_task *task = a->task;

    bool threw_exception = false;
    try {
        // The first argument is the return pointer; as the task fn 
        // must have void return type, we can safely pass 0.
        a->f(0, a->envptr, a->argptr);
    } catch (rust_task *ex) {
        A(task->sched, ex == task,
          "Expected this task to be thrown for unwinding");
        threw_exception = true;
    }

    rust_opaque_box* env = a->envptr;
    if(env) {
        // free the environment (which should be a unique closure).
        const type_desc *td = env->td;
        LOG(task, task, "Freeing env %p with td %p", env, td);
        td->drop_glue(NULL, NULL, td->first_param, box_body(env));
        upcall_free_shared_type_desc(env->td);
        upcall_shared_free(env);
    }

    // The cleanup work needs lots of stack
    cleanup_args ca = {a, threw_exception};
    task->sched->c_context.call_shim_on_c_stack(&ca, (void*)cleanup_task);

    task->ctx.next->swap(task->ctx);
}

void
rust_task::start(spawn_fn spawnee_fn,
                 rust_opaque_box *envptr,
                 void *argptr)
{
    LOG(this, task, "starting task from fn 0x%" PRIxPTR
        " with env 0x%" PRIxPTR " and arg 0x%" PRIxPTR,
        spawnee_fn, envptr, argptr);

    I(sched, stk->data != NULL);

    char *sp = (char *)user.rust_sp;

    sp -= sizeof(spawn_args);

    spawn_args *a = (spawn_args *)sp;

    a->task = this;
    a->envptr = envptr;
    a->argptr = argptr;
    a->f = spawnee_fn;

    ctx.call((void *)task_start_wrapper, a, sp);

    this->start();
}

void rust_task::start()
{
    transition(&sched->newborn_tasks, &sched->running_tasks);
}

// Only run this on the rust stack
void
rust_task::yield(bool *killed) {
    if (this->killed) {
        *killed = true;
    }

    // Return to the scheduler.
    ctx.next->swap(ctx);

    if (this->killed) {
        *killed = true;
    }
}

void
rust_task::kill() {
    if (dead()) {
        // Task is already dead, can't kill what's already dead.
        fail_parent();
        return;
    }

    // Note the distinction here: kill() is when you're in an upcall
    // from task A and want to force-fail task B, you do B->kill().
    // If you want to fail yourself you do self->fail().
    LOG(this, task, "killing task %s @0x%" PRIxPTR, name, this);
    // When the task next goes to yield or resume it will fail
    killed = true;
    // Unblock the task so it can unwind.
    unblock();

    LOG(this, task, "preparing to unwind task: 0x%" PRIxPTR, this);
    // run_on_resume(rust_unwind_glue);
}

extern "C" CDECL
bool rust_task_is_unwinding(rust_task *rt) {
    return rt->unwinding;
}

void
rust_task::fail() {
    // See note in ::kill() regarding who should call this.
    DLOG(sched, task, "task %s @0x%" PRIxPTR " failing", name, this);
    backtrace();
    unwinding = true;
#ifndef __WIN32__
    throw this;
#else
    die();
    conclude_failure();
    // FIXME: Need unwinding on windows. This will end up aborting
    sched->fail();
#endif
}

void
rust_task::conclude_failure() {
    fail_parent();
}

void
rust_task::fail_parent() {
    if (supervisor) {
        DLOG(sched, task,
             "task %s @0x%" PRIxPTR
             " propagating failure to supervisor %s @0x%" PRIxPTR,
             name, this, supervisor->name, supervisor);
        supervisor->kill();
    }
    // FIXME: implement unwinding again.
    if (NULL == supervisor && propagate_failure)
        sched->fail();
}

void
rust_task::unsupervise()
{
    if (supervisor) {
        DLOG(sched, task,
             "task %s @0x%" PRIxPTR
             " disconnecting from supervisor %s @0x%" PRIxPTR,
             name, this, supervisor->name, supervisor);
        supervisor->deref();
    }
    supervisor = NULL;
    propagate_failure = false;
}

frame_glue_fns*
rust_task::get_frame_glue_fns(uintptr_t fp) {
    fp -= sizeof(uintptr_t);
    return *((frame_glue_fns**) fp);
}

bool
rust_task::running()
{
    return state == &sched->running_tasks;
}

bool
rust_task::blocked()
{
    return state == &sched->blocked_tasks;
}

bool
rust_task::blocked_on(rust_cond *on)
{
    return blocked() && cond == on;
}

bool
rust_task::dead()
{
    return state == &sched->dead_tasks;
}

void *
rust_task::malloc(size_t sz, const char *tag, type_desc *td)
{
    return local_region.malloc(sz, tag);
}

void *
rust_task::realloc(void *data, size_t sz)
{
    return local_region.realloc(data, sz);
}

void
rust_task::free(void *p)
{
    local_region.free(p);
}

void
rust_task::transition(rust_task_list *src, rust_task_list *dst) {
    bool unlock = false;
    if(!sched->lock.lock_held_by_current_thread()) {
        unlock = true;
        sched->lock.lock();
    }
    DLOG(sched, task,
         "task %s " PTR " state change '%s' -> '%s' while in '%s'",
         name, (uintptr_t)this, src->name, dst->name, state->name);
    I(sched, state == src);
    src->remove(this);
    dst->append(this);
    state = dst;
    sched->lock.signal();
    if(unlock)
        sched->lock.unlock();
}

void
rust_task::block(rust_cond *on, const char* name) {
    I(sched, !lock.lock_held_by_current_thread());
    scoped_lock with(lock);
    LOG(this, task, "Blocking on 0x%" PRIxPTR ", cond: 0x%" PRIxPTR,
                         (uintptr_t) on, (uintptr_t) cond);
    A(sched, cond == NULL, "Cannot block an already blocked task.");
    A(sched, on != NULL, "Cannot block on a NULL object.");

    transition(&sched->running_tasks, &sched->blocked_tasks);
    cond = on;
    cond_name = name;
}

void
rust_task::wakeup(rust_cond *from) {
    I(sched, !lock.lock_held_by_current_thread());
    scoped_lock with(lock);
    A(sched, cond != NULL, "Cannot wake up unblocked task.");
    LOG(this, task, "Blocked on 0x%" PRIxPTR " woken up on 0x%" PRIxPTR,
                        (uintptr_t) cond, (uintptr_t) from);
    A(sched, cond == from, "Cannot wake up blocked task on wrong condition.");

    cond = NULL;
    cond_name = "none";
    transition(&sched->blocked_tasks, &sched->running_tasks);
}

void
rust_task::die() {
    I(sched, !lock.lock_held_by_current_thread());
    scoped_lock with(lock);
    transition(&sched->running_tasks, &sched->dead_tasks);
}

void
rust_task::unblock() {
    if (blocked()) {
        // FIXME: What if another thread unblocks the task between when
        // we checked and here?
        wakeup(cond);
    }
}

rust_crate_cache *
rust_task::get_crate_cache()
{
    if (!cache) {
        DLOG(sched, task, "fetching cache for current crate");
        cache = sched->get_cache();
    }
    return cache;
}

void
rust_task::backtrace() {
    if (!log_rt_backtrace) return;
#ifndef __WIN32__
    void *call_stack[256];
    int nframes = ::backtrace(call_stack, 256);
    backtrace_symbols_fd(call_stack + 1, nframes - 1, 2);
#endif
}

void *
rust_task::calloc(size_t size, const char *tag) {
    return local_region.calloc(size, tag);
}

rust_port_id rust_task::register_port(rust_port *port) {
    I(sched, !lock.lock_held_by_current_thread());
    scoped_lock with(lock);

    rust_port_id id = next_port_id++;
    port_table.put(id, port);
    return id;
}

void rust_task::release_port(rust_port_id id) {
    I(sched, lock.lock_held_by_current_thread());
    port_table.remove(id);
}

rust_port *rust_task::get_port_by_id(rust_port_id id) {
    I(sched, !lock.lock_held_by_current_thread());
    scoped_lock with(lock);
    rust_port *port = NULL;
    port_table.get(id, &port);
    if (port) {
        port->ref();
    }
    return port;
}

void
rust_task::notify(bool success) {
    // FIXME (1078) Do this in rust code
    if(user.notify_enabled) {
        rust_task *target_task = kernel->get_task_by_id(user.notify_chan.task);
        if (target_task) {
            rust_port *target_port =
                target_task->get_port_by_id(user.notify_chan.port);
            if(target_port) {
                task_notification msg;
                msg.id = user.id;
                msg.result = !success ? tr_failure : tr_success;

                target_port->send(&msg);
                scoped_lock with(target_task->lock);
                target_port->deref();
            }
            target_task->deref();
        }
    }
}

extern "C" CDECL void
record_sp(void *limit);

void *
rust_task::new_stack(size_t stk_sz, void *args_addr, size_t args_sz) {

    stk_seg *stk_seg = new_stk(sched, this, stk_sz + args_sz);
    A(sched, stk_seg->end - (uintptr_t)stk_seg->data >= stk_sz + args_sz,
      "Did not receive enough stack");
    uint8_t *new_sp = (uint8_t*)stk_seg->end;
    // Push the function arguments to the new stack
    new_sp = align_down(new_sp - args_sz);
    memcpy(new_sp, args_addr, args_sz);
    record_stack_limit();
    return new_sp;
}

void
rust_task::del_stack() {
    del_stk(this, stk);
    record_stack_limit();
}

void
rust_task::record_stack_limit() {
    // The function prolog compares the amount of stack needed to the end of
    // the stack. As an optimization, when the frame size is less than 256
    // bytes, it will simply compare %esp to to the stack limit instead of
    // subtracting the frame size. As a result we need our stack limit to
    // account for those 256 bytes.
    const unsigned LIMIT_OFFSET = 256;
    A(sched,
      (uintptr_t)stk->end - RED_ZONE_SIZE
      - (uintptr_t)stk->data >= LIMIT_OFFSET,
      "Stack size must be greater than LIMIT_OFFSET");
    record_sp(stk->data + LIMIT_OFFSET + RED_ZONE_SIZE);
}

extern "C" uintptr_t get_sp();

static bool
sp_in_stk_seg(uintptr_t sp, stk_seg *stk) {
    // Not positive these bounds for sp are correct.  I think that the first
    // possible value for esp on a new stack is stk->end, which points to the
    // address before the first value to be pushed onto a new stack. The last
    // possible address we can push data to is stk->data.  Regardless, there's
    // so much slop at either end that we should never hit one of these
    // boundaries.
    return (uintptr_t)stk->data <= sp && sp <= stk->end;
}

/*
Called by landing pads during unwinding to figure out which
stack segment we are currently running on, delete the others,
and record the stack limit (which was not restored when unwinding
through __morestack).
 */
void
rust_task::reset_stack_limit() {
    uintptr_t sp = get_sp();
    while (!sp_in_stk_seg(sp, stk)) {
        del_stk(this, stk);
        A(sched, stk != NULL, "Failed to find the current stack");
    }
    record_stack_limit();
}

/*
Returns true if we're currently running on the Rust stack
 */
bool
rust_task::on_rust_stack() {
    return sp_in_stk_seg(get_sp(), stk);
}

void
rust_task::check_stack_canary() {
    ::check_stack_canary(stk);
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
