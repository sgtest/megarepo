
#include "rust_internal.h"

#include "valgrind.h"
#include "memcheck.h"

#ifndef __WIN32__
#include <execinfo.h>
#endif

#include "globals.h"

// Stacks

// FIXME (issue #151): This should be 0x300; the change here is for
// practicality's sake until stack growth is working.
static size_t const min_stk_bytes = 0x300000;
// static size_t const min_stk_bytes = 0x10000;

// Task stack segments. Heap allocated and chained together.

static stk_seg*
new_stk(rust_dom *dom, size_t minsz)
{
    if (minsz < min_stk_bytes)
        minsz = min_stk_bytes;
    size_t sz = sizeof(stk_seg) + minsz;
    stk_seg *stk = (stk_seg *)dom->malloc(sz);
    LOGPTR(dom, "new stk", (uintptr_t)stk);
    memset(stk, 0, sizeof(stk_seg));
    stk->limit = (uintptr_t) &stk->data[minsz];
    LOGPTR(dom, "stk limit", stk->limit);
    stk->valgrind_id =
        VALGRIND_STACK_REGISTER(&stk->data[0],
                                &stk->data[minsz]);
    return stk;
}

static void
del_stk(rust_dom *dom, stk_seg *stk)
{
    VALGRIND_STACK_DEREGISTER(stk->valgrind_id);
    LOGPTR(dom, "freeing stk segment", (uintptr_t)stk);
    dom->free(stk);
}

// Tasks

// FIXME (issue #31): ifdef by platform. This is getting absurdly
// x86-specific.

size_t const n_callee_saves = 4;
size_t const callee_save_fp = 0;

rust_task::rust_task(rust_dom *dom, rust_task_list *state,
                     rust_task *spawner, const char *name) :
    maybe_proxy<rust_task>(this),
    stk(new_stk(dom, 0)),
    runtime_sp(0),
    rust_sp(stk->limit),
    gc_alloc_chain(0),
    dom(dom),
    cache(NULL),
    name(name),
    state(state),
    cond(NULL),
    cond_name("none"),
    supervisor(spawner),
    list_index(-1),
    rendezvous_ptr(0),
    alarm(this),
    handle(NULL)
{
    LOGPTR(dom, "new task", (uintptr_t)this);
    DLOG(dom, task, "sizeof(task) = %d (0x%x)", sizeof *this, sizeof *this);

    if (spawner == NULL) {
        ref_count = 0;
    }
}

rust_task::~rust_task()
{
    DLOG(dom, task, "~rust_task %s @0x%" PRIxPTR ", refcnt=%d",
         name, (uintptr_t)this, ref_count);

    /*
      for (uintptr_t fp = get_fp(); fp; fp = get_previous_fp(fp)) {
      frame_glue_fns *glue_fns = get_frame_glue_fns(fp);
      DLOG(dom, task,
      "~rust_task, frame fp=0x%" PRIxPTR ", glue_fns=0x%" PRIxPTR,
      fp, glue_fns);
      if (glue_fns) {
      DLOG(dom, task,
               "~rust_task, mark_glue=0x%" PRIxPTR,
               glue_fns->mark_glue);
      DLOG(dom, task,
               "~rust_task, drop_glue=0x%" PRIxPTR,
               glue_fns->drop_glue);
      DLOG(dom, task,
               "~rust_task, reloc_glue=0x%" PRIxPTR,
               glue_fns->reloc_glue);
      }
      }
    */

    /* FIXME: tighten this up, there are some more
       assertions that hold at task-lifecycle events. */
    I(dom, ref_count == 0 ||
      (ref_count == 1 && this == dom->root_task));

    del_stk(dom, stk);
}

extern "C" void rust_new_exit_task_glue();

struct spawn_args {
    rust_task *task;
    uintptr_t a3;
    uintptr_t a4;
    void (*CDECL f)(int *, rust_task *, 
                       uintptr_t, uintptr_t);
};

// TODO: rewrite this in LLVM assembly so we can be sure the calling
// conventions will match.
extern "C" CDECL
void task_start_wrapper(spawn_args *a)
{
    rust_task *task = a->task;
    int rval = 42;
    
    // This is used by the context switching code. LLVM generates fastcall
    // functions, but ucontext needs cdecl functions. This massages the
    // calling conventions into the right form.
    a->f(&rval, task, a->a3, a->a4);

    LOG(task, task, "task exited with value %d", rval);

    // TODO: the old exit glue does some magical argument copying stuff. This
    // is probably still needed.

    // This is duplicated from upcall_exit, which is probably dead code by
    // now.
    LOG(task, task, "task ref_count: %d", task->ref_count);
    A(task->dom, task->ref_count >= 0,
      "Task ref_count should not be negative on exit!");
    task->die();
    task->notify_tasks_waiting_to_join();
    task->yield(1);
}

void
rust_task::start(uintptr_t spawnee_fn,
                 uintptr_t args,
                 size_t callsz)
{
    LOGPTR(dom, "from spawnee", spawnee_fn);

    I(dom, stk->data != NULL);

    char *sp = (char *)stk->limit;

    sp -= sizeof(spawn_args);

    spawn_args *a = (spawn_args *)sp;

    a->task = this;
    a->a3 = 0xca11ab1e;
    a->a4 = args;
    void **f = (void **)&a->f;
    *f = (void *)spawnee_fn;

    ctx.call((void *)task_start_wrapper, a, sp);

    yield_timer.reset(0);
    transition(&dom->newborn_tasks, &dom->running_tasks);
}

void
rust_task::grow(size_t n_frame_bytes)
{
    // FIXME (issue #151): Just fail rather than almost certainly crashing
    // mysteriously later. The commented-out logic below won't work at all in
    // the presence of non-word-aligned pointers.
    abort();

}

void
rust_task::yield(size_t nargs) {
    yield(nargs, 0);
}

void
rust_task::yield(size_t nargs, size_t time_in_us) {
    LOG(this, task, "task %s @0x%" PRIxPTR " yielding for %d us",
        name, this, time_in_us);

    // TODO: what is nargs for, and is it safe to ignore?

    yield_timer.reset(time_in_us);

    // Return to the scheduler.
    ctx.next->swap(ctx);
}

void
rust_task::kill() {
    if (dead()) {
        // Task is already dead, can't kill what's already dead.
        return;
    }

    // Note the distinction here: kill() is when you're in an upcall
    // from task A and want to force-fail task B, you do B->kill().
    // If you want to fail yourself you do self->fail(upcall_nargs).
    LOG(this, task, "killing task %s @0x%" PRIxPTR, name, this);
    // Unblock the task so it can unwind.
    unblock();

    if (this == dom->root_task)
        dom->fail();

    LOG(this, task, "preparing to unwind task: 0x%" PRIxPTR, this);
    // run_on_resume(rust_unwind_glue);
}

void
rust_task::fail(size_t nargs) {
    // See note in ::kill() regarding who should call this.
    DLOG(dom, task, "task %s @0x%" PRIxPTR " failing", name, this);
    backtrace();
    // Unblock the task so it can unwind.
    unblock();
    if (this == dom->root_task)
        dom->fail();
    // run_after_return(nargs, rust_unwind_glue);
    if (supervisor) {
        DLOG(dom, task,
             "task %s @0x%" PRIxPTR
             " propagating failure to supervisor %s @0x%" PRIxPTR,
             name, this, supervisor->name, supervisor);
        supervisor->kill();
    }
    // FIXME: implement unwinding again.
    exit(1);
}

void
rust_task::gc(size_t nargs)
{
    DLOG(dom, task,
             "task %s @0x%" PRIxPTR " garbage collecting", name, this);
    // run_after_return(nargs, rust_gc_glue);
}

void
rust_task::unsupervise()
{
    DLOG(dom, task,
             "task %s @0x%" PRIxPTR
             " disconnecting from supervisor %s @0x%" PRIxPTR,
             name, this, supervisor->name, supervisor);
    supervisor = NULL;
}

void
rust_task::notify_tasks_waiting_to_join() {
    while (tasks_waiting_to_join.is_empty() == false) {
        LOG(this, task, "notify_tasks_waiting_to_join: %d",
            tasks_waiting_to_join.size());
        maybe_proxy<rust_task> *waiting_task = 0;
        tasks_waiting_to_join.pop(&waiting_task);
        if (waiting_task->is_proxy()) {
            notify_message::send(notify_message::WAKEUP, "wakeup",
                get_handle(), waiting_task->as_proxy()->handle());
            delete waiting_task;
        } else {
            rust_task *task = waiting_task->referent();
            if (task->blocked() == true) {
                task->wakeup(this);
            }
        }
    }
}

frame_glue_fns*
rust_task::get_frame_glue_fns(uintptr_t fp) {
    fp -= sizeof(uintptr_t);
    return *((frame_glue_fns**) fp);
}

bool
rust_task::running()
{
    return state == &dom->running_tasks;
}

bool
rust_task::blocked()
{
    return state == &dom->blocked_tasks;
}

bool
rust_task::blocked_on(rust_cond *on)
{
    return blocked() && cond == on;
}

bool
rust_task::dead()
{
    return state == &dom->dead_tasks;
}

void
rust_task::link_gc(gc_alloc *gcm) {
    I(dom, gcm->prev == NULL);
    I(dom, gcm->next == NULL);
    gcm->prev = NULL;
    gcm->next = gc_alloc_chain;
    gc_alloc_chain = gcm;
    if (gcm->next)
        gcm->next->prev = gcm;
}

void
rust_task::unlink_gc(gc_alloc *gcm) {
    if (gcm->prev)
        gcm->prev->next = gcm->next;
    if (gcm->next)
        gcm->next->prev = gcm->prev;
    if (gc_alloc_chain == gcm)
        gc_alloc_chain = gcm->next;
    gcm->prev = NULL;
    gcm->next = NULL;
}

void *
rust_task::malloc(size_t sz, type_desc *td)
{
    // FIXME: GC is disabled for now.
    // Effects, GC-memory classification are all wrong.
    td = NULL;

    if (td) {
        sz += sizeof(gc_alloc);
    }
    void *mem = dom->malloc(sz);
    if (!mem)
        return mem;
    if (td) {
        gc_alloc *gcm = (gc_alloc*) mem;
        DLOG(dom, task, "task %s @0x%" PRIxPTR
             " allocated %d GC bytes = 0x%" PRIxPTR,
             name, (uintptr_t)this, sz, gcm);
        memset((void*) gcm, 0, sizeof(gc_alloc));
        link_gc(gcm);
        gcm->ctrl_word = (uintptr_t)td;
        gc_alloc_accum += sz;
        mem = (void*) &(gcm->data);
    }
    return mem;;
}

void *
rust_task::realloc(void *data, size_t sz, bool is_gc)
{
    // FIXME: GC is disabled for now.
    // Effects, GC-memory classification are all wrong.
    is_gc = false;
    if (is_gc) {
        gc_alloc *gcm = (gc_alloc*)(((char *)data) - sizeof(gc_alloc));
        unlink_gc(gcm);
        sz += sizeof(gc_alloc);
        gcm = (gc_alloc*) dom->realloc((void*)gcm, sz);
        DLOG(dom, task, "task %s @0x%" PRIxPTR
             " reallocated %d GC bytes = 0x%" PRIxPTR,
             name, (uintptr_t)this, sz, gcm);
        if (!gcm)
            return gcm;
        link_gc(gcm);
        data = (void*) &(gcm->data);
    } else {
        data = dom->realloc(data, sz);
    }
    return data;
}

void
rust_task::free(void *p, bool is_gc)
{
    // FIXME: GC is disabled for now.
    // Effects, GC-memory classification are all wrong.
    is_gc = false;
    if (is_gc) {
        gc_alloc *gcm = (gc_alloc*)(((char *)p) - sizeof(gc_alloc));
        unlink_gc(gcm);
        DLOG(dom, mem,
             "task %s @0x%" PRIxPTR " freeing GC memory = 0x%" PRIxPTR,
             name, (uintptr_t)this, gcm);
        dom->free(gcm);
    } else {
        dom->free(p);
    }
}

void
rust_task::transition(rust_task_list *src, rust_task_list *dst) {
    DLOG(dom, task,
         "task %s " PTR " state change '%s' -> '%s' while in '%s'",
         name, (uintptr_t)this, src->name, dst->name, state->name);
    I(dom, state == src);
    src->remove(this);
    dst->append(this);
    state = dst;
}

void
rust_task::block(rust_cond *on, const char* name) {
    LOG(this, task, "Blocking on 0x%" PRIxPTR ", cond: 0x%" PRIxPTR,
                         (uintptr_t) on, (uintptr_t) cond);
    A(dom, cond == NULL, "Cannot block an already blocked task.");
    A(dom, on != NULL, "Cannot block on a NULL object.");

    transition(&dom->running_tasks, &dom->blocked_tasks);
    cond = on;
    cond_name = name;
}

void
rust_task::wakeup(rust_cond *from) {
    A(dom, cond != NULL, "Cannot wake up unblocked task.");
    LOG(this, task, "Blocked on 0x%" PRIxPTR " woken up on 0x%" PRIxPTR,
                        (uintptr_t) cond, (uintptr_t) from);
    A(dom, cond == from, "Cannot wake up blocked task on wrong condition.");

    transition(&dom->blocked_tasks, &dom->running_tasks);
    I(dom, cond == from);
    cond = NULL;
    cond_name = "none";
}

void
rust_task::die() {
    transition(&dom->running_tasks, &dom->dead_tasks);
}

void
rust_task::unblock() {
    if (blocked())
        wakeup(cond);
}

rust_crate_cache *
rust_task::get_crate_cache()
{
    if (!cache) {
        DLOG(dom, task, "fetching cache for current crate");
        cache = dom->get_cache();
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

rust_handle<rust_task> *
rust_task::get_handle() {
    if (handle == NULL) {
        handle = dom->kernel->get_task_handle(this);
    }
    return handle;
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
