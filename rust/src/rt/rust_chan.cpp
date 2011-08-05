#include "rust_internal.h"
#include "rust_chan.h"

/**
 * Create a new rust channel and associate it with the specified port.
 */
rust_chan::rust_chan(rust_kernel *kernel, rust_port *port,
                     size_t unit_sz)
    : ref_count(1),
      kernel(kernel),
      port(port),
      buffer(kernel, unit_sz) {
    if (port) {
        associate(port);
    }
    KLOG(kernel, comm, "new rust_chan(task=0x%" PRIxPTR
        ", port=0x%" PRIxPTR ") -> chan=0x%" PRIxPTR,
        (uintptr_t) task, (uintptr_t) port, (uintptr_t) this);
}

rust_chan::~rust_chan() {
    KLOG(kernel, comm, "del rust_chan(task=0x%" PRIxPTR ")",
         (uintptr_t) this);

    this->destroy();

    A(kernel, is_associated() == false,
      "Channel must be disassociated before being freed.");
}

/**
 * Link this channel with the specified port.
 */
void rust_chan::associate(rust_port *port) {
    this->port = port;
    scoped_lock with(port->lock);
    KLOG(kernel, task,
         "associating chan: 0x%" PRIxPTR " with port: 0x%" PRIxPTR,
         this, port);
    this->ref();
    this->task = port->task;
    this->task->ref();
    this->port->chans.push(this);
}

bool rust_chan::is_associated() {
    return port != NULL;
}

/**
 * Unlink this channel from its associated port.
 */
void rust_chan::disassociate() {
    A(kernel,
      port->lock.lock_held_by_current_thread(),
      "Port referent lock must be held to call rust_chan::disassociate");
    A(kernel, is_associated(),
      "Channel must be associated with a port.");
    KLOG(kernel, task,
         "disassociating chan: 0x%" PRIxPTR " from port: 0x%" PRIxPTR,
         this, port);
    task->deref();
    this->task = NULL;
    port->chans.swap_delete(this);

    // Delete reference to the port.
    port = NULL;

    this->deref();
}

/**
 * Attempt to send data to the associated port.
 */
void rust_chan::send(void *sptr) {
    scoped_lock with(port->lock);

    buffer.enqueue(sptr);

    if (!is_associated()) {
        W(kernel, is_associated(),
          "rust_chan::transmit with no associated port.");
        return;
    }

    A(kernel, !buffer.is_empty(),
      "rust_chan::transmit with nothing to send.");

    if (port->task->blocked_on(port)) {
        KLOG(kernel, comm, "dequeued in rendezvous_ptr");
        buffer.dequeue(port->task->rendezvous_ptr);
        port->task->rendezvous_ptr = 0;
        port->task->wakeup(port);
    }
}

rust_chan *rust_chan::clone(rust_task *target) {
    return new (target->kernel, "cloned chan")
        rust_chan(kernel, port, buffer.unit_sz);
}

/**
 * Cannot Yield: If the task were to unwind, the dropped ref would still
 * appear to be live, causing modify-after-free errors.
 */
void rust_chan::destroy() {
    if (is_associated()) {
        // We're trying to delete a channel that another task may be
        // reading from. We have two options:
        //
        // 1. We can flush the channel by blocking in upcall_flush_chan()
        //    and resuming only when the channel is flushed. The problem
        //    here is that we can get ourselves in a deadlock if the
        //    parent task tries to join us.
        //
        // 2. We can leave the channel in a "dormnat" state by not freeing
        //    it and letting the receiver task delete it for us instead.
        if (buffer.is_empty() == false) {
            return;
        }
        scoped_lock with(port->lock);
        disassociate();
    }
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
