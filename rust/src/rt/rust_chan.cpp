#include "rust_internal.h"
#include "rust_chan.h"

/**
 * Create a new rust channel and associate it with the specified port.
 */
rust_chan::rust_chan(rust_task *task, maybe_proxy<rust_port> *port) :
    task(task), port(port), buffer(task->dom, port->delegate()->unit_sz) {

    if (port) {
        associate(port);
    }

    task->log(rust_log::MEM | rust_log::COMM,
              "new rust_chan(task=0x%" PRIxPTR
              ", port=0x%" PRIxPTR ") -> chan=0x%" PRIxPTR,
              (uintptr_t) task, (uintptr_t) port, (uintptr_t) this);
}

rust_chan::~rust_chan() {
    task->log(rust_log::MEM | rust_log::COMM,
              "del rust_chan(task=0x%" PRIxPTR ")", (uintptr_t) this);

    A(task->dom, is_associated() == false,
      "Channel must be disassociated before being freed.");
}

/**
 * Link this channel with the specified port.
 */
void rust_chan::associate(maybe_proxy<rust_port> *port) {
    this->port = port;
    if (port->is_proxy() == false) {
        task->log(rust_log::TASK,
            "associating chan: 0x%" PRIxPTR " with port: 0x%" PRIxPTR,
            this, port);
        this->port->delegate()->chans.push(this);
    }
}

bool rust_chan::is_associated() {
    return port != NULL;
}

/**
 * Unlink this channel from its associated port.
 */
void rust_chan::disassociate() {
    A(task->dom, is_associated(), "Channel must be associated with a port.");

    if (port->is_proxy() == false) {
        task->log(rust_log::TASK,
            "disassociating chan: 0x%" PRIxPTR " from port: 0x%" PRIxPTR,
            this, port->delegate());
        port->delegate()->chans.swap_delete(this);
    }

    // Delete reference to the port.
    port = NULL;
}

/**
 * Attempt to send data to the associated port.
 */
void rust_chan::send(void *sptr) {
    buffer.enqueue(sptr);

    rust_dom *dom = task->dom;
    if (!is_associated()) {
        W(dom, is_associated(),
          "rust_chan::transmit with no associated port.");
        return;
    }

    A(dom, !buffer.is_empty(),
      "rust_chan::transmit with nothing to send.");

    if (port->is_proxy()) {
        // TODO: Cache port task locally.
        rust_proxy<rust_task> *port_task =
            dom->get_task_proxy(port->delegate()->task);
        data_message::send(buffer.peek(), buffer.unit_sz,
            "send data", task, port_task, port->as_proxy());
        buffer.dequeue(NULL);
    } else {
        rust_port *target_port = port->delegate();
        if (target_port->task->blocked_on(target_port)) {
            dom->log(rust_log::COMM, "dequeued in rendezvous_ptr");
            buffer.dequeue(target_port->task->rendezvous_ptr);
            target_port->task->rendezvous_ptr = 0;
            target_port->task->wakeup(target_port);
            return;
        }
    }

    return;
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
