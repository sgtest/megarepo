#include "rust_port.h"
#include "rust_port_selector.h"

rust_port_selector::rust_port_selector()
    : ports(NULL), n_ports(0) {
}

void
rust_port_selector::select(rust_task *task, rust_port **dptr,
                           rust_port **ports,
                           size_t n_ports, uintptr_t *yield) {

    I(task->thread, this->ports == NULL);
    I(task->thread, this->n_ports == 0);
    I(task->thread, dptr != NULL);
    I(task->thread, ports != NULL);
    I(task->thread, n_ports != 0);
    I(task->thread, yield != NULL);

    *yield = false;
    size_t locks_taken = 0;
    bool found_msg = false;

    // Take each port's lock as we iterate through them because
    // if none of them contain a usable message then we need to
    // block the task before any of them can try to send another
    // message.

    // Start looking for ports from a different index each time.
    size_t j = isaac_rand(&task->thread->rctx);
    for (size_t i = 0; i < n_ports; i++) {
        size_t k = (i + j) % n_ports;
        rust_port *port = ports[k];
        I(task->thread, port != NULL);

        port->lock.lock();
        locks_taken++;

        if (port->buffer.size() > 0) {
            *dptr = port;
            found_msg = true;
            break;
        }
    }

    if (!found_msg) {
        this->ports = ports;
        this->n_ports = n_ports;
        I(task->thread, task->rendezvous_ptr == NULL);
        task->rendezvous_ptr = (uintptr_t*)dptr;
        task->block(this, "waiting for select rendezvous");

        // Blocking the task might fail if the task has already been
        // killed, but in the event of both failure and success the
        // task needs to yield. On success, it yields and waits to be
        // unblocked. On failure it yields and is then fails the task.

        *yield = true;
    }

    for (size_t i = 0; i < locks_taken; i++) {
        size_t k = (i + j) % n_ports;
        rust_port *port = ports[k];
        port->lock.unlock();
    }
}

void
rust_port_selector::msg_sent_on(rust_port *port) {
    rust_task *task = port->task;

    // Prevent two ports from trying to wake up the task
    // simultaneously
    scoped_lock with(rendezvous_lock);

    if (task->blocked_on(this)) {
        for (size_t i = 0; i < n_ports; i++) {
            if (port == ports[i]) {
                // This was one of the ports we were waiting on
                ports = NULL;
                n_ports = 0;
                *task->rendezvous_ptr = (uintptr_t) port;
                task->rendezvous_ptr = NULL;
                task->wakeup(this);
                return;
            }
        }
    }
}
