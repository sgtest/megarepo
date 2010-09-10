#include "rust_internal.h"

rust_kernel::rust_kernel(rust_srv *srv) :
    _region(&srv->local_region),
    _log(srv, NULL),
    _srv(srv),
    _interrupt_kernel_loop(FALSE),
    domains(&srv->local_region),
    message_queues(&srv->local_region) {
    // Nop.
}

rust_handle<rust_dom> *
rust_kernel::create_domain(const rust_crate *crate, const char *name) {
    _kernel_lock.lock();
    rust_message_queue *message_queue =
        new (this) rust_message_queue(_srv, this);
    rust_srv *srv = _srv->clone();
    rust_dom *dom =
        new (this) rust_dom(this, message_queue, srv, crate, name);
    rust_handle<rust_dom> *handle = internal_get_dom_handle(dom);
    message_queue->associate(handle);
    domains.append(dom);
    message_queues.append(message_queue);
    _kernel_lock.signal_all();
    _kernel_lock.unlock();
    return handle;
}

void
rust_kernel::destroy_domain(rust_dom *dom) {
    _kernel_lock.lock();
    log(rust_log::KERN, "deleting domain: " PTR ", index: %d, domains %d",
        dom, dom->list_index, domains.length());
    domains.remove(dom);
    dom->message_queue->disassociate();
    rust_srv *srv = dom->srv;
    delete dom;
    delete srv;
    _kernel_lock.signal_all();
    _kernel_lock.unlock();
}

rust_handle<rust_dom> *
rust_kernel::internal_get_dom_handle(rust_dom *dom) {
    rust_handle<rust_dom> *handle = NULL;
    if (_dom_handles.get(dom, &handle) == false) {
        handle =
            new (this) rust_handle<rust_dom>(this, dom->message_queue, dom);
        _dom_handles.put(dom, handle);
    }
    return handle;
}

rust_handle<rust_dom> *
rust_kernel::get_dom_handle(rust_dom *dom) {
    _kernel_lock.lock();
    rust_handle<rust_dom> *handle = internal_get_dom_handle(dom);
    _kernel_lock.unlock();
    return handle;
}

rust_handle<rust_task> *
rust_kernel::get_task_handle(rust_task *task) {
    _kernel_lock.lock();
    rust_handle<rust_task> *handle = NULL;
    if (_task_handles.get(task, &handle) == false) {
        handle =
            new (this) rust_handle<rust_task>(this, task->dom->message_queue,
                                              task);
        _task_handles.put(task, handle);
    }
    _kernel_lock.unlock();
    return handle;
}

rust_handle<rust_port> *
rust_kernel::get_port_handle(rust_port *port) {
    _kernel_lock.lock();
    rust_handle<rust_port> *handle = NULL;
    if (_port_handles.get(port, &handle) == false) {
        handle =
            new (this) rust_handle<rust_port>(this,
                                              port->task->dom->message_queue,
                                              port);
        _port_handles.put(port, handle);
    }
    _kernel_lock.unlock();
    return handle;
}

void
rust_kernel::join_all_domains() {
    _kernel_lock.lock();
    while (domains.length() > 0) {
        _kernel_lock.wait();
    }
    _kernel_lock.unlock();
    log(rust_log::KERN, "joined domains");
}

void
rust_kernel::log_all_domain_state() {
    log(rust_log::KERN, "log_all_domain_state: %d domains", domains.length());
    for (uint32_t i = 0; i < domains.length(); i++) {
        domains[i]->log_state();
    }
}

/**
 * Checks for simple deadlocks.
 */
bool
rust_kernel::is_deadlocked() {
    return false;
}

void
rust_kernel::log(uint32_t type_bits, char const *fmt, ...) {
    char buf[256];
    if (_log.is_tracing(type_bits)) {
        va_list args;
        va_start(args, fmt);
        vsnprintf(buf, sizeof(buf), fmt, args);
        _log.trace_ln(NULL, type_bits, buf);
        va_end(args);
    }
}

void
rust_kernel::pump_message_queues() {
    for (size_t i = 0; i < message_queues.length(); i++) {
        rust_message_queue *queue = message_queues[i];
        if (queue->is_associated() == false) {
            rust_message *message = NULL;
            while (queue->dequeue(&message)) {
                message->kernel_process();
                delete message;
            }
        }
    }
}

void
rust_kernel::start_kernel_loop() {
    _kernel_lock.lock();
    while (_interrupt_kernel_loop == false) {
        _kernel_lock.wait();
        pump_message_queues();
    }
    _kernel_lock.unlock();
}

void
rust_kernel::run() {
    log(rust_log::KERN, "started kernel loop");
    start_kernel_loop();
    log(rust_log::KERN, "finished kernel loop");
}

void
rust_kernel::terminate_kernel_loop() {
    log(rust_log::KERN, "terminating kernel loop");
    _interrupt_kernel_loop = true;
    _kernel_lock.lock();
    _kernel_lock.signal_all();
    _kernel_lock.unlock();
    join();
}

rust_kernel::~rust_kernel() {
    K(_srv, domains.length() == 0,
      "Kernel has %d live domain(s), join all domains before killing "
       "the kernel.", domains.length());

    terminate_kernel_loop();

    // It's possible that the message pump misses some messages because
    // of races, so pump any remaining messages here. By now all domain
    // threads should have been joined, so we shouldn't miss any more
    // messages.
    pump_message_queues();

    log(rust_log::KERN, "freeing handles");

    free_handles(_task_handles);
    free_handles(_port_handles);
    free_handles(_dom_handles);

    log(rust_log::KERN, "freeing queues");

    rust_message_queue *queue = NULL;
    while (message_queues.pop(&queue)) {
        K(_srv, queue->is_empty(), "Kernel message queue should be empty "
          "before killing the kernel.");
        delete queue;
    }
}

void *
rust_kernel::malloc(size_t size) {
    return _region->malloc(size);
}

void rust_kernel::free(void *mem) {
    _region->free(mem);
}

template<class T> void
rust_kernel::free_handles(hash_map<T*, rust_handle<T>* > &map) {
    T* key;
    rust_handle<T> *value;
    while (map.pop(&key, &value)) {
        delete value;
    }
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
