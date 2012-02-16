#include "rust_internal.h"
#include "rust_port.h"


rust_port::rust_port(rust_task *task, size_t unit_sz)
    : ref_count(1), kernel(task->kernel), task(task),
      unit_sz(unit_sz), buffer(kernel, unit_sz) {

    LOG(task, comm,
        "new rust_port(task=0x%" PRIxPTR ", unit_sz=%d) -> port=0x%"
        PRIxPTR, (uintptr_t)task, unit_sz, (uintptr_t)this);

    task->ref();
    id = task->register_port(this);
}

rust_port::~rust_port() {
    LOG(task, comm, "~rust_port 0x%" PRIxPTR, (uintptr_t) this);

    task->deref();
}

void rust_port::detach() {
    I(task->thread, !task->lock.lock_held_by_current_thread());
    scoped_lock with(task->lock);
    {
        task->release_port(id);
    }
}

void rust_port::send(void *sptr) {
    I(task->thread, !lock.lock_held_by_current_thread());
    bool did_rendezvous = false;
    {
        scoped_lock with(lock);

        buffer.enqueue(sptr);

        A(kernel, !buffer.is_empty(),
          "rust_chan::transmit with nothing to send.");

        if (task->blocked_on(this)) {
            KLOG(kernel, comm, "dequeued in rendezvous_ptr");
            buffer.dequeue(task->rendezvous_ptr);
            task->rendezvous_ptr = 0;
            task->wakeup(this);
            did_rendezvous = true;
        }
    }

    if (!did_rendezvous) {
        // If the task wasn't waiting specifically on this port,
        // it may be waiting on a group of ports

        rust_port_selector *port_selector = task->get_port_selector();
        // This check is not definitive. The port selector will take a lock
        // and check again whether the task is still blocked.
        if (task->blocked_on(port_selector)) {
            port_selector->msg_sent_on(this);
        }
    }
}

bool rust_port::receive(void *dptr) {
    I(task->thread, lock.lock_held_by_current_thread());
    if (buffer.is_empty() == false) {
        buffer.dequeue(dptr);
        LOG(task, comm, "<=== read data ===");
        return true;
    }
    return false;
}

size_t rust_port::size() {
    I(task->thread, !lock.lock_held_by_current_thread());
    scoped_lock with(lock);
    return buffer.size();
}

void rust_port::log_state() {
    LOG(task, comm,
        "port size: %d",
        buffer.size());
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
