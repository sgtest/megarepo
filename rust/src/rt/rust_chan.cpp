#include "rust_internal.h"
#include "rust_chan.h"

/**
 * Create a new rust channel and associate it with the specified port.
 */
rust_chan::rust_chan(rust_task *task,
                     maybe_proxy<rust_port> *port,
                     size_t unit_sz) 
    : ref_count(1),
      kernel(task->kernel),
      task(task),
      port(port),
      buffer(task, unit_sz) {
    ++task->ref_count;
    if (port) {
        associate(port);
    }
    LOG(task, comm, "new rust_chan(task=0x%" PRIxPTR
        ", port=0x%" PRIxPTR ") -> chan=0x%" PRIxPTR,
        (uintptr_t) task, (uintptr_t) port, (uintptr_t) this);
}

rust_chan::~rust_chan() {
    LOG(task, comm, "del rust_chan(task=0x%" PRIxPTR ")", (uintptr_t) this);

    A(task->sched, is_associated() == false,
      "Channel must be disassociated before being freed.");
    --task->ref_count;
}

/**
 * Link this channel with the specified port.
 */
void rust_chan::associate(maybe_proxy<rust_port> *port) {
    this->port = port;
    if (port->is_proxy() == false) {
        LOG(task, task,
            "associating chan: 0x%" PRIxPTR " with port: 0x%" PRIxPTR,
            this, port);
        ++this->ref_count;
        this->port->referent()->chans.push(this);
    }
}

bool rust_chan::is_associated() {
    return port != NULL;
}

/**
 * Unlink this channel from its associated port.
 */
void rust_chan::disassociate() {
    A(task->sched, is_associated(),
      "Channel must be associated with a port.");

    if (port->is_proxy() == false) {
        LOG(task, task,
            "disassociating chan: 0x%" PRIxPTR " from port: 0x%" PRIxPTR,
            this, port->referent());
        --this->ref_count;
        port->referent()->chans.swap_delete(this);
    }

    // Delete reference to the port.
    port = NULL;
}

/**
 * Attempt to send data to the associated port.
 */
void rust_chan::send(void *sptr) {
    buffer.enqueue(sptr);

    rust_scheduler *sched = task->sched;
    if (!is_associated()) {
        W(sched, is_associated(),
          "rust_chan::transmit with no associated port.");
        return;
    }

    A(sched, !buffer.is_empty(),
      "rust_chan::transmit with nothing to send.");

    if (port->is_proxy()) {
        data_message::send(buffer.peek(), buffer.unit_sz, "send data",
                           task->get_handle(), port->as_proxy()->handle());
        buffer.dequeue(NULL);
    } else {
        rust_port *target_port = port->referent();
        scoped_lock with(target_port->lock);
        if (target_port->task->blocked_on(target_port)) {
            DLOG(sched, comm, "dequeued in rendezvous_ptr");
            buffer.dequeue(target_port->task->rendezvous_ptr);
            target_port->task->rendezvous_ptr = 0;
            target_port->task->wakeup(target_port);
            return;
        }
    }

    return;
}

rust_chan *rust_chan::clone(maybe_proxy<rust_task> *target) {
    size_t unit_sz = buffer.unit_sz;
    maybe_proxy<rust_port> *port = this->port;
    rust_task *target_task = NULL;
    if (target->is_proxy() == false) {
        port = this->port;
        target_task = target->referent();
    } else {
        rust_handle<rust_port> *handle =
            task->sched->kernel->get_port_handle(port->as_referent());
        maybe_proxy<rust_port> *proxy = new rust_proxy<rust_port> (handle);
        LOG(task, mem, "new proxy: " PTR, proxy);
        port = proxy;
        target_task = target->as_proxy()->handle()->referent();
    }
    return new (target_task->kernel) rust_chan(target_task, port, unit_sz);
}

/**
 * Cannot Yield: If the task were to unwind, the dropped ref would still
 * appear to be live, causing modify-after-free errors.
 */
void rust_chan::destroy() {
    A(task->sched, ref_count == 0,
      "Channel's ref count should be zero.");

    if (is_associated()) {
        if (port->is_proxy()) {
            // Here is a good place to delete the port proxy we allocated
            // in upcall_clone_chan.
            rust_proxy<rust_port> *proxy = port->as_proxy();
            disassociate();
            delete proxy;
        } else {
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
            disassociate();
        }
    }
    delete this;
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
