#ifndef RUST_CHAN_H
#define RUST_CHAN_H

class rust_chan : public rc_base<rust_chan>,
                  public task_owned<rust_chan>,
                  public rust_cond {
public:
    rust_chan(rust_task *task, maybe_proxy<rust_port> *port);
    ~rust_chan();

    rust_task *task;
    maybe_proxy<rust_port> *port;
    size_t idx;
    circular_buffer buffer;

    void associate(maybe_proxy<rust_port> *port);
    void disassociate();
    bool is_associated();

    void send(void *sptr);
};

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

#endif /* RUST_CHAN_H */
