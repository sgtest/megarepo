#include "rust_internal.h"

// Upcalls.

#ifdef __GNUC__
#define LOG_UPCALL_ENTRY(task)                              \
    (task)->dom->get_log().reset_indent(0);                 \
    (task)->log(rust_log::UPCALL,                           \
                "> UPCALL %s - task: %s 0x%" PRIxPTR        \
                " retpc: x%" PRIxPTR                        \
                " ref_count: %d",                           \
                __FUNCTION__,                               \
                (task)->name, (task),                       \
                __builtin_return_address(0),                \
                (task->ref_count));                         \
    (task)->dom->get_log().indent();
#else
#define LOG_UPCALL_ENTRY(task)                              \
    (task)->dom->get_log().reset_indent(0);                 \
    (task)->log(rust_log::UPCALL,                           \
                "> UPCALL task: %s @x%" PRIxPTR,            \
                (task)->name, (task));                      \
    (task)->dom->get_log().indent();
#endif

void
log_task_state(rust_task *task, maybe_proxy<rust_task> *target) {
    rust_task *delegate = target->delegate();
    if (target->is_proxy()) {
        task->log(rust_log::TASK,
                  "remote task: 0x%" PRIxPTR ", ref_count: %d state: %s",
                  delegate, delegate->ref_count, delegate->state_str());
    } else {
        task->log(rust_log::TASK,
                  "local task: 0x%" PRIxPTR ", ref_count: %d state: %s",
                  delegate, delegate->ref_count, delegate->state_str());
    }
}

extern "C" CDECL char const *str_buf(rust_task *task, rust_str *s);

extern "C" void upcall_grow_task(rust_task *task, size_t n_frame_bytes) {
    LOG_UPCALL_ENTRY(task);
    task->grow(n_frame_bytes);
}

extern "C" CDECL void upcall_log_int(rust_task *task, int32_t i) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::ULOG,
              "rust: %" PRId32 " (0x%" PRIx32 ")", i, i);
}

extern "C" CDECL void upcall_log_str(rust_task *task, rust_str *str) {
    LOG_UPCALL_ENTRY(task);
    const char *c = str_buf(task, str);
    task->log(rust_log::UPCALL | rust_log::ULOG, "rust: %s", c);
}

extern "C" CDECL void upcall_trace_word(rust_task *task, uintptr_t i) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::TRACE, "trace: 0x%" PRIxPTR "", i,
              i, (char) i);
}

extern "C" CDECL void upcall_trace_str(rust_task *task, char const *c) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::TRACE, "trace: %s", c);
}

extern "C" CDECL rust_port*
upcall_new_port(rust_task *task, size_t unit_sz) {
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;
    task->log(rust_log::UPCALL | rust_log::MEM | rust_log::COMM,
              "upcall_new_port(task=0x%" PRIxPTR " (%s), unit_sz=%d)",
              (uintptr_t) task, task->name, unit_sz);
    return new (dom) rust_port(task, unit_sz);
}

extern "C" CDECL void upcall_del_port(rust_task *task, rust_port *port) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::MEM | rust_log::COMM,
              "upcall del_port(0x%" PRIxPTR ")", (uintptr_t) port);
    I(task->dom, !port->ref_count);
    delete port;
}

/**
 * Creates a new channel pointing to a given port.
 */
extern "C" CDECL rust_chan*
upcall_new_chan(rust_task *task, rust_port *port) {
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;
    task->log(rust_log::UPCALL | rust_log::MEM | rust_log::COMM,
              "upcall_new_chan("
              "task=0x%" PRIxPTR " (%s), port=0x%" PRIxPTR ")",
              (uintptr_t) task, task->name, port);
    I(dom, port);
    return new (dom) rust_chan(task, port);
}

/**
 * Called whenever this channel needs to be flushed. This can happen due to a
 * flush statement, or automatically whenever a channel's ref count is
 * about to drop to zero.
 */
extern "C" CDECL void
upcall_flush_chan(rust_task *task, rust_chan *chan) {
    LOG_UPCALL_ENTRY(task);
    // Nop.
}

/**
 * Called whenever the channel's ref count drops to zero.
 *
 * Cannot Yield: If the task were to unwind, the dropped ref would still
 * appear to be live, causing modify-after-free errors.
 */
extern "C" CDECL void upcall_del_chan(rust_task *task, rust_chan *chan) {
    LOG_UPCALL_ENTRY(task);

    task->log(rust_log::UPCALL | rust_log::MEM | rust_log::COMM,
              "upcall del_chan(0x%" PRIxPTR ")", (uintptr_t) chan);

    A(task->dom, chan->ref_count == 0,
      "Channel's ref count should be zero.");

    if (chan->is_associated()) {
        // We're trying to delete a channel that another task may be reading
        // from. We have two options:
        //
        // 1. We can flush the channel by blocking in upcall_flush_chan()
        //    and resuming only when the channel is flushed. The problem
        //    here is that we can get ourselves in a deadlock if the parent
        //    task tries to join us.
        //
        // 2. We can leave the channel in a "dormnat" state by not freeing
        //    it and letting the receiver task delete it for us instead.
        if (chan->buffer.is_empty() == false) {
            return;
        }
        chan->disassociate();
    }
    delete chan;
}

/**
 * Clones a channel and stores it in the spawnee's domain. Each spawned task
 * has it's own copy of the channel.
 */
extern "C" CDECL rust_chan *
upcall_clone_chan(rust_task *task,
                  maybe_proxy<rust_task> *target,
                  rust_chan *chan) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::COMM,
              "target: 0x%" PRIxPTR ", chan: 0x%" PRIxPTR,
              target, chan);
    rust_task *target_task = target->delegate();
    maybe_proxy<rust_port> *port = chan->port;
    if (target->is_proxy()) {
        port = target_task->dom->get_port_proxy_synchronized(
                chan->port->as_delegate());
    }
    return new (target_task->dom) rust_chan(target_task, port);
}

extern "C" CDECL void upcall_yield(rust_task *task) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::COMM, "upcall yield()");
    task->yield(1);
}

extern "C" CDECL void upcall_sleep(rust_task *task, size_t time_in_us) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::TASK, "elapsed %d",
              task->yield_timer.get_elapsed_time());
    task->log(rust_log::UPCALL | rust_log::TASK, "sleep %d us", time_in_us);
    task->yield(2, time_in_us);
}

extern "C" CDECL void
upcall_join(rust_task *task, maybe_proxy<rust_task> *target) {
    LOG_UPCALL_ENTRY(task);
    rust_task *target_task = target->delegate();
    task->log(rust_log::UPCALL | rust_log::COMM,
              "target: 0x%" PRIxPTR ", task: %s @0x%" PRIxPTR,
              target, target_task->name, target_task);

    if (target->is_proxy()) {
        notify_message::
        send(notify_message::JOIN, "join", task, target->as_proxy());
        task->block(target_task, "joining remote task");
        task->yield(2);
    } else {
        // If the other task is already dying, we don't have to wait for it.
        if (target_task->dead() == false) {
            target_task->tasks_waiting_to_join.push(task);
            task->block(target_task, "joining local task");
            task->yield(2);
        }
    }
}

/**
 * Buffers a chunk of data in the specified channel.
 *
 * sptr: pointer to a chunk of data to buffer
 */
extern "C" CDECL void
upcall_send(rust_task *task, rust_chan *chan, void *sptr) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::COMM,
              "chan: 0x%" PRIxPTR ", sptr: 0x%" PRIxPTR ", size: %d",
              (uintptr_t) chan, (uintptr_t) sptr,
              chan->port->delegate()->unit_sz);
    chan->send(sptr);
    task->log(rust_log::COMM, "=== sent data ===>");
}

extern "C" CDECL void
upcall_recv(rust_task *task, uintptr_t *dptr, rust_port *port) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::COMM,
              "port: 0x%" PRIxPTR ", dptr: 0x%" PRIxPTR
              ", size: 0x%" PRIxPTR ", chan_no: %d",
              (uintptr_t) port, (uintptr_t) dptr, port->unit_sz,
              port->chans.length());

    if (port->receive(dptr)) {
        return;
    }

    // No data was buffered on any incoming channel, so block this task
    // on the port. Remember the rendezvous location so that any sender
    // task can write to it before waking up this task.

    task->log(rust_log::COMM, "<=== waiting for rendezvous data ===");
    task->rendezvous_ptr = dptr;
    task->block(port, "waiting for rendezvous data");
    task->yield(3);
}

extern "C" CDECL void upcall_fail(rust_task *task, char const *expr,
    char const *file, size_t line) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::ERR,
              "upcall fail '%s', %s:%" PRIdPTR, expr, file, line);
    task->fail(4);
}

/**
 * Called whenever a task's ref count drops to zero.
 */
extern "C" CDECL void
upcall_kill(rust_task *task, maybe_proxy<rust_task> *target) {
    LOG_UPCALL_ENTRY(task);
    log_task_state(task, target);
    rust_task *target_task = target->delegate();

    task->log(rust_log::UPCALL | rust_log::TASK,
              "kill task %s @0x%" PRIxPTR ", ref count %d",
              target_task->name, target_task,
              target_task->ref_count);

    if (target->is_proxy()) {
        notify_message::
        send(notify_message::KILL, "kill", task, target->as_proxy());
        // The proxy ref_count dropped to zero, delete it here.
        delete target->as_proxy();
    } else {
        target_task->kill();
    }
}

/**
 * Called by the exit glue when the task terminates.
 */
extern "C" CDECL void
upcall_exit(rust_task *task) {
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::TASK,
              "task ref_count: %d", task->ref_count);
    A(task->dom, task->ref_count >= 0,
      "Task ref_count should not be negative on exit!");
    task->die();
    task->notify_tasks_waiting_to_join();
    task->yield(1);
}

extern "C" CDECL uintptr_t
upcall_malloc(rust_task *task, size_t nbytes, type_desc *td)
{
    LOG_UPCALL_ENTRY(task);

    task->dom->log(rust_log::UPCALL|rust_log::MEM,
                   "upcall malloc(%" PRIdPTR ", 0x%" PRIxPTR ")"
                   " with gc-chain head = 0x%" PRIxPTR,
                   nbytes, td, task->gc_alloc_chain);
    void *p = task->malloc(nbytes, td);
    task->dom->log(rust_log::UPCALL|rust_log::MEM,
                   "upcall malloc(%" PRIdPTR ", 0x%" PRIxPTR
                   ") = 0x%" PRIxPTR
                   " with gc-chain head = 0x%" PRIxPTR,
                   nbytes, td, (uintptr_t)p, task->gc_alloc_chain);
    return (uintptr_t) p;
}

/**
 * Called whenever an object's ref count drops to zero.
 */
extern "C" CDECL void
upcall_free(rust_task *task, void* ptr, uintptr_t is_gc)
{
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;
    dom->log(rust_log::UPCALL|rust_log::MEM,
             "upcall free(0x%" PRIxPTR ")",
             (uintptr_t)ptr);
    task->free(ptr, (bool) is_gc);
}

extern "C" CDECL uintptr_t
upcall_mark(rust_task *task, void* ptr)
{
    LOG_UPCALL_ENTRY(task);

    rust_dom *dom = task->dom;
    if (ptr) {
        gc_alloc *gcm = (gc_alloc*) (((char*)ptr) - sizeof(gc_alloc));
        uintptr_t marked = (uintptr_t) gcm->mark();
        dom->log(rust_log::UPCALL|rust_log::MEM|rust_log::GC,
                 "upcall mark(0x%" PRIxPTR ") = %" PRIdPTR,
                 (uintptr_t)gcm, marked);
        return marked;
    }
    return 0;
}

extern "C" CDECL rust_str *
upcall_new_str(rust_task *task, char const *s, size_t fill) {
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;
    size_t alloc = next_power_of_two(sizeof(rust_str) + fill);
    void *mem = dom->malloc(alloc);
    if (!mem) {
        task->fail(3);
        return NULL;
    }
    rust_str *st = new (mem) rust_str(dom, alloc, fill, (uint8_t const *) s);
    task->log(rust_log::UPCALL | rust_log::MEM,
              "upcall new_str('%s', %" PRIdPTR ") = 0x%" PRIxPTR,
              s, fill, st);
    return st;
}

extern "C" CDECL rust_vec *
upcall_new_vec(rust_task *task, size_t fill, type_desc *td)
{
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;
    dom->log(rust_log::UPCALL|rust_log::MEM,
             "upcall new_vec(%" PRIdPTR ")",
             fill);
    size_t alloc = next_power_of_two(sizeof(rust_vec) + fill);
    void *mem = task->malloc(alloc, td);
    if (!mem) {
        task->fail(3);
        return NULL;
    }
    rust_vec *v = new (mem) rust_vec(dom, alloc, 0, NULL);
    task->log(rust_log::UPCALL | rust_log::MEM,
              "upcall new_vec(%" PRIdPTR ") = 0x%" PRIxPTR, fill, v);
    return v;
}

extern "C" CDECL rust_str *
upcall_vec_grow(rust_task *task, rust_vec *v, size_t n_bytes, uintptr_t is_gc)
{
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;
    task->log(rust_log::UPCALL|rust_log::MEM,
             "upcall vec_grow(%" PRIxPTR ", %" PRIdPTR
             "), alloc=%" PRIdPTR ", fill=%" PRIdPTR,
             v, n_bytes, v->alloc, v->fill);
    size_t alloc = next_power_of_two(sizeof(rust_vec) + v->fill + n_bytes);
    if (v->ref_count == 1) {

        // Fastest path: already large enough.
        if (v->alloc >= alloc) {
            task->log(rust_log::UPCALL | rust_log::MEM, "no-growth path");
            return v;
        }

        // Second-fastest path: can at least realloc.
        task->log(rust_log::UPCALL | rust_log::MEM, "realloc path");
        v = (rust_vec*) dom->realloc(v, alloc);
        if (!v) {
            task->fail(4);
            return NULL;
        }
        v->alloc = alloc;

    } else {
        // Slowest path: make a new vec.
        task->log(rust_log::UPCALL | rust_log::MEM, "new vec path");
        void *mem = dom->malloc(alloc);
        if (!mem) {
            task->fail(4);
            return NULL;
        }
        v->deref();
        v = new (mem) rust_vec(dom, alloc, v->fill, &v->data[0]);
    }
    I(dom, sizeof(rust_vec) + v->fill <= v->alloc);
    return v;
}

static rust_crate_cache::c_sym *
fetch_c_sym(rust_task *task, rust_crate const *curr_crate, size_t lib_num,
    size_t c_sym_num, char const *library, char const *symbol) {
    rust_crate_cache *cache = task->get_crate_cache(curr_crate);
    rust_crate_cache::lib *l = cache->get_lib(lib_num, library);
    return cache->get_c_sym(c_sym_num, l, symbol);
}

extern "C" CDECL uintptr_t upcall_require_rust_sym(rust_task *task,
    rust_crate const *curr_crate, size_t lib_num, // # of lib
    size_t c_sym_num, // # of C sym "rust_crate" in lib
    size_t rust_sym_num, // # of rust sym
    char const *library, char const **path) {
    LOG_UPCALL_ENTRY(task);
    rust_dom *dom = task->dom;

    task->log(rust_log::UPCALL | rust_log::CACHE,
              "upcall require rust sym: lib #%" PRIdPTR
              " = %s, c_sym #%" PRIdPTR
              ", rust_sym #%" PRIdPTR
              ", curr_crate = 0x%" PRIxPTR, lib_num, library, c_sym_num,
              rust_sym_num, curr_crate);
    for (char const **c = crate_rel(curr_crate, path); *c; ++c) {
        task->log(rust_log::UPCALL, " + %s", crate_rel(curr_crate, *c));
    }

    task->log(rust_log::UPCALL | rust_log::CACHE,
              "require C symbol 'rust_crate' from lib #%" PRIdPTR, lib_num);
    rust_crate_cache::c_sym *c =
            fetch_c_sym(task, curr_crate, lib_num, c_sym_num, library,
                        "rust_crate");

    task->log(rust_log::UPCALL | rust_log::CACHE,
              "require rust symbol inside crate");
    rust_crate_cache::rust_sym *s = task->cache->get_rust_sym(rust_sym_num,
                                                              dom,
                                                              curr_crate, c,
                                                              path);

    uintptr_t addr = s->get_val();
    if (addr) {
        task->log(rust_log::UPCALL | rust_log::CACHE,
                  "found-or-cached addr: 0x%" PRIxPTR, addr);
    } else {
        task->log(rust_log::UPCALL | rust_log::CACHE,
                  "failed to resolve symbol");
        task->fail(7);
    }
    return addr;
}

extern "C" CDECL uintptr_t upcall_require_c_sym(rust_task *task,
    rust_crate const *curr_crate, size_t lib_num, // # of lib
    size_t c_sym_num, // # of C sym
    char const *library, char const *symbol) {
    LOG_UPCALL_ENTRY(task);

    task->log(rust_log::UPCALL | rust_log::CACHE,
              "upcall require c sym: lib #%" PRIdPTR
              " = %s, c_sym #%" PRIdPTR
              " = %s"
              ", curr_crate = 0x%" PRIxPTR, lib_num, library, c_sym_num,
              symbol, curr_crate);

    rust_crate_cache::c_sym *c = fetch_c_sym(task, curr_crate, lib_num,
                                             c_sym_num, library, symbol);

    uintptr_t addr = c->get_val();
    if (addr) {
        task->log(rust_log::UPCALL | rust_log::CACHE,
                  "found-or-cached addr: 0x%" PRIxPTR, addr);
    } else {
        task->log(rust_log::UPCALL | rust_log::CACHE,
                  "failed to resolve symbol");
        task->fail(6);
    }
    return addr;
}

extern "C" CDECL type_desc *
upcall_get_type_desc(rust_task *task,
                     rust_crate const *curr_crate,
                     size_t size,
                     size_t align,
                     size_t n_descs,
                     type_desc const **descs)
{
    LOG_UPCALL_ENTRY(task);
    task->log(rust_log::UPCALL | rust_log::CACHE,
              "upcall get_type_desc with size=%" PRIdPTR
              ", align=%" PRIdPTR ", %" PRIdPTR " descs", size, align,
              n_descs);
    rust_crate_cache *cache = task->get_crate_cache(curr_crate);
    type_desc *td = cache->get_type_desc(size, align, n_descs, descs);
    task->log(rust_log::UPCALL | rust_log::CACHE,
              "returning tydesc 0x%" PRIxPTR, td);
    return td;
}

#if defined(__WIN32__)
static DWORD WINAPI rust_thread_start(void *ptr)
#elif defined(__GNUC__)
static void *rust_thread_start(void *ptr)
#else
#error "Platform not supported"
#endif
{
    // We were handed the domain we are supposed to run.
    rust_dom *dom = (rust_dom *) ptr;

    // Start a new rust main loop for this thread.
    dom->start_main_loop();

    rust_srv *srv = dom->srv;
    delete dom;
    delete srv;

    return 0;
}

extern "C" CDECL rust_task *
upcall_new_task(rust_task *spawner, const char *name) {
    LOG_UPCALL_ENTRY(spawner);

    rust_dom *dom = spawner->dom;
    rust_task *task = new (dom) rust_task(dom, spawner, name);
    dom->log(rust_log::UPCALL | rust_log::MEM | rust_log::TASK,
             "upcall new_task(spawner %s @0x%" PRIxPTR ", %s) = 0x%" PRIxPTR,
             spawner->name, spawner, name, task);
    return task;
}

extern "C" CDECL rust_task *
upcall_start_task(rust_task *spawner, rust_task *task,
    uintptr_t exit_task_glue, uintptr_t spawnee_fn, size_t callsz) {
    LOG_UPCALL_ENTRY(spawner);

    rust_dom *dom = spawner->dom;
    dom->log(rust_log::UPCALL | rust_log::MEM | rust_log::TASK,
             "upcall start_task(task %s @0x%" PRIxPTR
             " exit_task_glue 0x%" PRIxPTR
             ", spawnee 0x%" PRIxPTR
             ", callsz %" PRIdPTR ")", task->name, task, exit_task_glue,
             spawnee_fn, callsz);
    task->start(exit_task_glue, spawnee_fn, spawner->rust_sp, callsz);
    return task;
}

extern "C" CDECL maybe_proxy<rust_task> *
upcall_new_thread(rust_task *task, const char *name) {
    LOG_UPCALL_ENTRY(task);

    rust_dom *old_dom = task->dom;
    rust_dom *new_dom = new rust_dom(old_dom->srv->clone(),
                                     old_dom->root_crate,
                                     name);

    task->log(rust_log::UPCALL | rust_log::MEM,
              "upcall new_thread(%s) = dom 0x%" PRIxPTR " task 0x%" PRIxPTR,
              name, new_dom, new_dom->root_task);
    rust_proxy<rust_task> *proxy =
        new (old_dom) rust_proxy<rust_task>(old_dom,
                                            new_dom->root_task, true);
    task->log(rust_log::UPCALL | rust_log::MEM,
              "new proxy = 0x%" PRIxPTR " -> task = 0x%" PRIxPTR,
              proxy, proxy->delegate());
    return proxy;
}

extern "C" CDECL maybe_proxy<rust_task> *
upcall_start_thread(rust_task *spawner,
                    maybe_proxy<rust_task> *root_task_proxy,
    uintptr_t exit_task_glue, uintptr_t spawnee_fn, size_t callsz) {
    LOG_UPCALL_ENTRY(spawner);

    rust_dom *dom = spawner->dom;
    rust_task *root_task = root_task_proxy->delegate();
    dom->log(rust_log::UPCALL | rust_log::MEM | rust_log::TASK,
             "upcall start_thread(exit_task_glue 0x%" PRIxPTR
             ", spawnee 0x%" PRIxPTR
             ", callsz %" PRIdPTR ")", exit_task_glue, spawnee_fn, callsz);
    root_task->start(exit_task_glue, spawnee_fn, spawner->rust_sp, callsz);

#if defined(__WIN32__)
    HANDLE thread;
    thread = CreateThread(NULL, 0, rust_thread_start, root_task->dom,
            0, NULL);
    dom->win32_require("CreateThread", thread != NULL);
#else
    pthread_t thread;
    pthread_create(&thread, &dom->attr, rust_thread_start,
                   (void *) root_task->dom);
#endif
    return root_task_proxy;
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
