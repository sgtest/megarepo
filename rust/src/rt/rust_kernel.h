#ifndef RUST_KERNEL_H
#define RUST_KERNEL_H

/**
 * A handle object for Rust tasks. We need a reference to the message queue
 * of the referent's domain which we can safely hang on to since it's a
 * kernel object. We use the referent reference as a label we stash in
 * messages sent via this proxy.
 */

class rust_kernel;
class rust_message;

template <typename T> class
rust_handle :
    public rust_cond,
    public rc_base<rust_handle<T> >,
    public kernel_owned<rust_handle<T> > {
public:
    rust_kernel *kernel;
    rust_message_queue *message_queue;
    T *_referent;
    T * referent() {
        return _referent;
    }
    rust_handle(rust_kernel *kernel,
                rust_message_queue *message_queue,
                T *referent) :
                kernel(kernel),
                message_queue(message_queue),
                _referent(referent) {
        // Nop.
    }
};

/**
 * A global object shared by all thread domains. Most of the data structures
 * in this class are synchronized since they are accessed from multiple
 * threads.
 */
class rust_kernel : public rust_thread {
    memory_region *_region;
    rust_log _log;
    rust_srv *_srv;

    /**
     * Task proxy objects are kernel owned handles to Rust objects.
     */
    hash_map<rust_task *, rust_handle<rust_task> *> _task_handles;
    hash_map<rust_port *, rust_handle<rust_port> *> _port_handles;
    hash_map<rust_dom *, rust_handle<rust_dom> *> _dom_handles;

    template<class T> void free_handles(hash_map<T*, rust_handle<T>* > &map);

    void run();
    void start_kernel_loop();
    bool _interrupt_kernel_loop;

    lock_and_signal _kernel_lock;

    void terminate_kernel_loop();
    void pump_message_queues();

    rust_handle<rust_dom> *internal_get_dom_handle(rust_dom *dom);

public:

    /**
     * List of domains that are currently executing.
     */
    indexed_list<rust_dom> domains;

    /**
     * Message queues are kernel objects and are associated with domains.
     * Their lifetime is not bound to the lifetime of a domain and in fact
     * live on after their associated domain has died. This way we can safely
     * communicate with domains that may have died.
     *
     */
    indexed_list<rust_message_queue> message_queues;

    rust_handle<rust_dom> *get_dom_handle(rust_dom *dom);
    rust_handle<rust_task> *get_task_handle(rust_task *task);
    rust_handle<rust_port> *get_port_handle(rust_port *port);

    rust_kernel(rust_srv *srv);

    rust_handle<rust_dom> *create_domain(rust_crate const *root_crate,
                                         const char *name);
    void destroy_domain(rust_dom *dom);

    bool is_deadlocked();

    void signal_kernel_lock();

    /**
     * Notifies the kernel whenever a message has been enqueued . This gives
     * the kernel the opportunity to wake up the message pump thread if the
     * message queue is not associated.
     */
    void
    notify_message_enqueued(rust_message_queue *queue, rust_message *message);

    /**
     * Blocks until all domains have terminated.
     */
    void join_all_domains();

    void log_all_domain_state();
    void log(uint32_t level, char const *fmt, ...);
    virtual ~rust_kernel();

    void *malloc(size_t size);
    void free(void *mem);
};

inline void *operator new(size_t size, rust_kernel *kernel) {
    return kernel->malloc(size);
}

inline void *operator new(size_t size, rust_kernel &kernel) {
    return kernel.malloc(size);
}

#endif /* RUST_KERNEL_H */
