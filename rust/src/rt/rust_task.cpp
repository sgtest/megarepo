
#include "rust_internal.h"

#include "valgrind.h"
#include "memcheck.h"

#ifndef __WIN32__
#include <execinfo.h>
#endif

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
    dom->logptr("new stk", (uintptr_t)stk);
    memset(stk, 0, sizeof(stk_seg));
    stk->limit = (uintptr_t) &stk->data[minsz];
    dom->logptr("stk limit", stk->limit);
    stk->valgrind_id =
        VALGRIND_STACK_REGISTER(&stk->data[0],
                                &stk->data[minsz]);
    return stk;
}

static void
del_stk(rust_dom *dom, stk_seg *stk)
{
    VALGRIND_STACK_DEREGISTER(stk->valgrind_id);
    dom->logptr("freeing stk segment", (uintptr_t)stk);
    dom->free(stk);
}

// Tasks

// FIXME (issue #31): ifdef by platform. This is getting absurdly
// x86-specific.

size_t const n_callee_saves = 4;
size_t const callee_save_fp = 0;

static uintptr_t
align_down(uintptr_t sp)
{
    // There is no platform we care about that needs more than a
    // 16-byte alignment.
    return sp & ~(16 - 1);
}

static uintptr_t*
align_down(uintptr_t* sp)
{
    return (uintptr_t*) align_down((uintptr_t)sp);
}


static void
make_aligned_room_for_bytes(uintptr_t*& sp, size_t n)
{
    uintptr_t tmp = (uintptr_t) sp;
    tmp = align_down(tmp - n) + n;
    sp = (uintptr_t*) tmp;
}


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
    dom->logptr("new task", (uintptr_t)this);

    if (spawner == NULL) {
        ref_count = 0;
    }
}

rust_task::~rust_task()
{
    dom->log(rust_log::MEM|rust_log::TASK,
             "~rust_task %s @0x%" PRIxPTR ", refcnt=%d",
             name, (uintptr_t)this, ref_count);

    /*
      for (uintptr_t fp = get_fp(); fp; fp = get_previous_fp(fp)) {
      frame_glue_fns *glue_fns = get_frame_glue_fns(fp);
      dom->log(rust_log::MEM|rust_log::TASK,
      "~rust_task, frame fp=0x%" PRIxPTR ", glue_fns=0x%" PRIxPTR,
      fp, glue_fns);
      if (glue_fns) {
      dom->log(rust_log::MEM|rust_log::TASK,
               "~rust_task, mark_glue=0x%" PRIxPTR,
               glue_fns->mark_glue);
      dom->log(rust_log::MEM|rust_log::TASK,
               "~rust_task, drop_glue=0x%" PRIxPTR,
               glue_fns->drop_glue);
      dom->log(rust_log::MEM|rust_log::TASK,
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
    if (cache)
        cache->deref();
}

void
rust_task::start(uintptr_t exit_task_glue,
                 uintptr_t spawnee_abi,
                 uintptr_t spawnee_fn,
                 uintptr_t args,
                 size_t callsz)
{
    dom->logptr("exit-task glue", exit_task_glue);
    dom->logptr("from spawnee", spawnee_fn);

    // Set sp to last uintptr_t-sized cell of segment
    rust_sp -= sizeof(uintptr_t);

    // NB: Darwin needs "16-byte aligned" stacks *at the point of the call
    // instruction in the caller*. This means that the address at which a
    // retpc is pushed must always be 16-byte aligned.
    //
    // see: "Mac OS X ABI Function Call Guide"


    // Begin synthesizing frames. There are two: a "fully formed"
    // exit-task frame at the top of the stack -- that pretends to be
    // mid-execution -- and a just-starting frame beneath it that
    // starts executing the first instruction of the spawnee. The
    // spawnee *thinks* it was called by the exit-task frame above
    // it. It wasn't; we put that fake frame in place here, but the
    // illusion is enough for the spawnee to return to the exit-task
    // frame when it's done, and exit.
    uintptr_t *spp = (uintptr_t *)rust_sp;


    // The exit_task_glue frame we synthesize above the frame we activate:
    make_aligned_room_for_bytes(spp, 3 * sizeof(uintptr_t));
    *spp-- = (uintptr_t) 0;          // closure-or-obj
    *spp-- = (uintptr_t) this;       // task
    *spp-- = (uintptr_t) 0x0;        // output
    I(dom, spp == align_down(spp));
    *spp-- = (uintptr_t) 0x0;        // retpc

    uintptr_t exit_task_frame_base;

    if (spawnee_abi == ABI_X86_RUSTBOOT_CDECL) {
        for (size_t j = 0; j < n_callee_saves; ++j) {

            // We want 'frame_base' to point to the old fp in this (exit-task)
            // frame, because we're going to inject this frame-pointer into
            // the callee-save frame pointer value in the *next* (spawnee)
            // frame. A cheap trick, but this means the spawnee frame will
            // restore the proper frame pointer of the glue frame as it runs
            // its epilogue.
            if (j == callee_save_fp)
                exit_task_frame_base = (uintptr_t)spp;

            *spp-- = 0;
        }

        *spp-- = (uintptr_t) dom->root_crate;  // crate ptr
        *spp-- = (uintptr_t) 0;                // frame_glue_fns
    }

    I(dom, args);
    if (spawnee_abi == ABI_X86_RUSTBOOT_CDECL)
        make_aligned_room_for_bytes(spp, callsz);
    else
        make_aligned_room_for_bytes(spp, callsz - 2 * sizeof(uintptr_t));

    // Copy args from spawner to spawnee.
    uintptr_t *src = (uintptr_t *)args;
    src += 1;                  // spawn-call output slot
    src += 1;                  // spawn-call task slot
    src += 1;                  // spawn-call closure-or-obj slot

    // Undo previous sp-- so we're pointing at the last word pushed.
    ++spp;

    // Memcpy all but the task, output and env pointers
    callsz -= (3 * sizeof(uintptr_t));
    spp = (uintptr_t*) (((uintptr_t)spp) - callsz);
    memcpy(spp, src, callsz);

    // Move sp down to point to last implicit-arg cell (env).
    spp--;

    // The *implicit* incoming args to the spawnee frame we're
    // activating:
    *spp-- = (uintptr_t) 0x0;               // closure-or-obj

    if (spawnee_abi == ABI_X86_RUSTBOOT_CDECL) {
        // in CDECL mode we write the task + outptr to the spawnee stack.
        *spp-- = (uintptr_t) this;            // task
        *spp-- = (uintptr_t) 0;               // output addr
    } else {
        // in FASTCALL mode we don't, the outptr will be in ecx and the task
        // in edx, and the activate_glue will make sure to set that up.
        I(dom, spawnee_abi == ABI_X86_RUSTC_FASTCALL);
    }

    I(dom, spp == align_down(spp));
    *spp-- = (uintptr_t) exit_task_glue;  // retpc

    // The context the activate_glue needs to switch stack.
    *spp-- = (uintptr_t) spawnee_fn;      // instruction to start at
    for (size_t j = 0; j < n_callee_saves; ++j) {
        // callee-saves to carry in when we activate
        if (j == callee_save_fp)
            *spp-- = exit_task_frame_base;
        else
            *spp-- = (uintptr_t)NULL;
    }

    // Back up one, we overshot where sp should be.
    rust_sp = (uintptr_t) (spp+1);

    transition(&dom->newborn_tasks, &dom->running_tasks);
}

void
rust_task::grow(size_t n_frame_bytes)
{
    // FIXME (issue #151): Just fail rather than almost certainly crashing
    // mysteriously later. The commented-out logic below won't work at all in
    // the presence of non-word-aligned pointers.
    abort();

#if 0
    stk_seg *old_stk = this->stk;
    uintptr_t old_top = (uintptr_t) old_stk->limit;
    uintptr_t old_bottom = (uintptr_t) &old_stk->data[0];
    uintptr_t rust_sp_disp = old_top - this->rust_sp;
    size_t ssz = old_top - old_bottom;
    dom->log(rust_log::MEM|rust_log::TASK|rust_log::UPCALL,
             "upcall_grow_task(%" PRIdPTR
             "), old size %" PRIdPTR
             " bytes (old lim: 0x%" PRIxPTR ")",
             n_frame_bytes, ssz, old_top);
    ssz *= 2;
    if (ssz < n_frame_bytes)
        ssz = n_frame_bytes;
    ssz = next_power_of_two(ssz);

    dom->log(rust_log::MEM|rust_log::TASK, "upcall_grow_task growing stk 0x%"
             PRIxPTR " to %d bytes", old_stk, ssz);

    stk_seg *nstk = new_stk(dom, ssz);
    uintptr_t new_top = (uintptr_t) &nstk->data[ssz];
    size_t n_copy = old_top - old_bottom;
    dom->log(rust_log::MEM|rust_log::TASK,
             "copying %d bytes of stack from [0x%" PRIxPTR ", 0x%" PRIxPTR "]"
             " to [0x%" PRIxPTR ", 0x%" PRIxPTR "]",
             n_copy,
             old_bottom, old_bottom + n_copy,
             new_top - n_copy, new_top);

    VALGRIND_MAKE_MEM_DEFINED((void*)old_bottom, n_copy);
    memcpy((void*)(new_top - n_copy), (void*)old_bottom, n_copy);

    nstk->limit = new_top;
    this->stk = nstk;
    this->rust_sp = new_top - rust_sp_disp;

    dom->log(rust_log::MEM|rust_log::TASK, "processing relocations");

    // FIXME (issue #32): this is the most ridiculously crude
    // relocation scheme ever. Try actually, you know, writing out
    // reloc descriptors?
    size_t n_relocs = 0;
    for (uintptr_t* p = (uintptr_t*)(new_top - n_copy);
         p < (uintptr_t*)new_top; ++p) {
        if (old_bottom <= *p && *p < old_top) {
            //dom->log(rust_log::MEM, "relocating pointer 0x%" PRIxPTR
            //        " by %d bytes", *p, (new_top - old_top));
            n_relocs++;
            *p += (new_top - old_top);
        }
    }
    dom->log(rust_log::MEM|rust_log::TASK,
             "processed %d relocations", n_relocs);
    del_stk(dom, old_stk);
    dom->logptr("grown stk limit", new_top);
#endif
}

void
push_onto_thread_stack(uintptr_t &sp, uintptr_t value)
{
    asm("xchgl %0, %%esp\n"
        "push %2\n"
        "xchgl %0, %%esp\n"
        : "=r" (sp)
        : "0" (sp), "r" (value)
        : "eax");
}

void
rust_task::run_after_return(size_t nargs, uintptr_t glue)
{
    // This is only safe to call if we're the currently-running task.
    check_active();

    uintptr_t sp = runtime_sp;

    // The compiler reserves nargs + 1 word for oldsp on the stack and
    // then aligns it.
    sp = align_down(sp - nargs * sizeof(uintptr_t));

    uintptr_t *retpc = ((uintptr_t *) sp) - 1;
    dom->log(rust_log::TASK|rust_log::MEM,
             "run_after_return: overwriting retpc=0x%" PRIxPTR
             " @ runtime_sp=0x%" PRIxPTR
             " with glue=0x%" PRIxPTR,
             *retpc, sp, glue);

    // Move the current return address (which points into rust code)
    // onto the rust stack and pretend we just called into the glue.
    push_onto_thread_stack(rust_sp, *retpc);
    *retpc = glue;
}

void
rust_task::run_on_resume(uintptr_t glue)
{
    // This is only safe to call if we're suspended.
    check_suspended();

    // Inject glue as resume address in the suspended frame.
    uintptr_t* rsp = (uintptr_t*) rust_sp;
    rsp += n_callee_saves;
    dom->log(rust_log::TASK|rust_log::MEM,
             "run_on_resume: overwriting retpc=0x%" PRIxPTR
             " @ rust_sp=0x%" PRIxPTR
             " with glue=0x%" PRIxPTR,
             *rsp, rsp, glue);
    *rsp = glue;
}

void
rust_task::yield(size_t nargs) {
    yield(nargs, 0);
}

void
rust_task::yield(size_t nargs, size_t time_in_us) {
    log(rust_log::TASK,
        "task %s @0x%" PRIxPTR " yielding for %d us",
        name, this, time_in_us);
    yield_timer.reset(time_in_us);
    run_after_return(nargs, dom->root_crate->get_yield_glue());
}

static inline uintptr_t
get_callee_save_fp(uintptr_t *top_of_callee_saves)
{
    return top_of_callee_saves[n_callee_saves - (callee_save_fp + 1)];
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
    log(rust_log::TASK, "killing task %s @0x%" PRIxPTR, name, this);
    // Unblock the task so it can unwind.
    unblock();

    if (this == dom->root_task)
        dom->fail();

    log(rust_log::TASK, "preparing to unwind task: 0x%" PRIxPTR, this);
    run_on_resume(dom->root_crate->get_unwind_glue());
}

void
rust_task::fail(size_t nargs) {
    // See note in ::kill() regarding who should call this.
    dom->log(rust_log::TASK, "task %s @0x%" PRIxPTR " failing", name, this);
    backtrace();
    // Unblock the task so it can unwind.
    unblock();
    if (this == dom->root_task)
        dom->fail();
    run_after_return(nargs, dom->root_crate->get_unwind_glue());
    if (supervisor) {
        dom->log(rust_log::TASK,
                 "task %s @0x%" PRIxPTR
                 " propagating failure to supervisor %s @0x%" PRIxPTR,
                 name, this, supervisor->name, supervisor);
        supervisor->kill();
    }
}

void
rust_task::gc(size_t nargs)
{
    dom->log(rust_log::TASK|rust_log::MEM,
             "task %s @0x%" PRIxPTR " garbage collecting", name, this);
    run_after_return(nargs, dom->root_crate->get_gc_glue());
}

void
rust_task::unsupervise()
{
    dom->log(rust_log::TASK,
             "task %s @0x%" PRIxPTR
             " disconnecting from supervisor %s @0x%" PRIxPTR,
             name, this, supervisor->name, supervisor);
    supervisor = NULL;
}

void
rust_task::notify_tasks_waiting_to_join() {
    while (tasks_waiting_to_join.is_empty() == false) {
        log(rust_log::TASK, "notify_tasks_waiting_to_join: %d",
            tasks_waiting_to_join.size());
        maybe_proxy<rust_task> *waiting_task;
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

uintptr_t
rust_task::get_fp() {
    // sp in any suspended task points to the last callee-saved reg on
    // the task stack.
    return get_callee_save_fp((uintptr_t*)rust_sp);
}

uintptr_t
rust_task::get_previous_fp(uintptr_t fp) {
    // FIXME: terribly X86-specific.
    // *fp == previous_fp.
    return *((uintptr_t*)fp);
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
        dom->log(rust_log::TASK|rust_log::MEM|rust_log::GC,
                 "task %s @0x%" PRIxPTR
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
        dom->log(rust_log::TASK|rust_log::MEM|rust_log::GC,
                 "task %s @0x%" PRIxPTR
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
        dom->log(rust_log::TASK|rust_log::MEM|rust_log::GC,
                 "task %s @0x%" PRIxPTR " freeing GC memory = 0x%" PRIxPTR,
                 name, (uintptr_t)this, gcm);
        dom->free(gcm);
    } else {
        dom->free(p);
    }
}

void
rust_task::transition(rust_task_list *src, rust_task_list *dst) {
    I(dom, state == src);
    dom->log(rust_log::TASK,
             "task %s " PTR " state change '%s' -> '%s'",
             name, (uintptr_t)this, src->name, dst->name);
    src->remove(this);
    dst->append(this);
    state = dst;
}

void
rust_task::block(rust_cond *on, const char* name) {
    log(rust_log::TASK, "Blocking on 0x%" PRIxPTR ", cond: 0x%" PRIxPTR,
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
    log(rust_log::TASK, "Blocked on 0x%" PRIxPTR " woken up on 0x%" PRIxPTR,
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
rust_task::get_crate_cache(rust_crate const *curr_crate)
{
    if (cache && cache->crate != curr_crate) {
        dom->log(rust_log::TASK, "switching task crate-cache to crate 0x%"
                 PRIxPTR, curr_crate);
        cache->deref();
        cache = NULL;
    }

    if (!cache) {
        dom->log(rust_log::TASK, "fetching cache for current crate");
        cache = dom->get_cache(curr_crate);
    }
    return cache;
}

void
rust_task::log(uint32_t type_bits, char const *fmt, ...) {
    char buf[BUF_BYTES];
    if (dom->get_log().is_tracing(type_bits)) {
        va_list args;
        va_start(args, fmt);
        vsnprintf(buf, sizeof(buf), fmt, args);
        dom->get_log().trace_ln(this, type_bits, buf);
        va_end(args);
    }
}

void
rust_task::backtrace() {
    if (!dom->get_log().is_tracing(rust_log::BT))
        return;

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
