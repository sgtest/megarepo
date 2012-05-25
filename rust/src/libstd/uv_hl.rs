#[doc = "
High-level bindings to work with the libuv library.

This module is geared towards library developers who want to
provide a high-level, abstracted interface to some set of
libuv functionality.
"];

export high_level_loop;
export spawn_high_level_loop;
export interact;
export exit;

import libc::c_void;
import ptr::addr_of;
import comm::{port, chan, methods};
import ll = uv_ll;

#[doc = "
Used to abstract-away direct interaction with a libuv loop.
"]
enum high_level_loop = {
    async_handle: *ll::uv_async_t,
    op_chan: chan<high_level_msg>
};

fn spawn_high_level_loop(-builder: task::builder
                        ) -> high_level_loop unsafe {

    import task::{set_opts, get_opts, single_threaded, run};

    let hll_po = port::<high_level_loop>();
    let hll_ch = hll_po.chan();

    set_opts(builder, {
        sched: some({
            mode: single_threaded,
            native_stack_size: none
        })
        with get_opts(builder)
    });

    run(builder) {||
        #debug("entering libuv task");
        run_high_level_loop(hll_ch);
        #debug("libuv task exiting");
    };

    hll_po.recv()
}

#[doc="
Represents the range of interactions with a `high_level_loop`
"]
enum high_level_msg {
    interaction (fn~(*libc::c_void)),
    #[doc="
For use in libraries that roll their own `high_level_loop` (like
`std::uv::global_loop`)

Is used to signal to the loop that it should close the internally-held
async handle and do a sanity check to make sure that all other handles are
closed, causing a failure otherwise. This should not be sent/used from
'normal' user code.
    "]
    teardown_loop
}

#[doc = "
Useful for anyone who wants to roll their own `high_level_loop`.
"]
unsafe fn run_high_level_loop(hll_ch: chan<high_level_loop>) {
    let msg_po = port::<high_level_msg>();
    let loop_ptr = ll::loop_new();
    // set up the special async handle we'll use to allow multi-task
    // communication with this loop
    let async = ll::async_t();
    let async_handle = addr_of(async);
    // associate the async handle with the loop
    ll::async_init(loop_ptr, async_handle, high_level_wake_up_cb);

    // initialize our loop data and store it in the loop
    let data: hl_loop_data = {
        async_handle: async_handle,
        mut active: true,
        msg_po_ptr: addr_of(msg_po)
    };
    ll::set_data_for_uv_handle(async_handle, addr_of(data));

    // Send out a handle through which folks can talk to us
    // while we dwell in the I/O loop
    let hll = high_level_loop({
        async_handle: async_handle,
        op_chan: msg_po.chan()
    });
    hll_ch.send(hll);

    log(debug, "about to run high level loop");
    // enter the loop... this blocks until the loop is done..
    ll::run(loop_ptr);
    log(debug, "high-level loop ended");
    ll::loop_delete(loop_ptr);
}

#[doc = "
Provide a callback to be processed by `a_loop`

The primary way to do operations again a running `high_level_loop` that
doesn't involve creating a uv handle via `safe_handle`

# Warning

This function is the only safe way to interact with _any_ `high_level_loop`.
Using functions in the `uv::ll` module outside of the `cb` passed into
this function is _very dangerous_.

# Arguments

* hl_loop - a `uv::hl::high_level_loop` that you want to do operations against
* cb - a function callback to be processed on the running loop's
thread. The only parameter passed in is an opaque pointer representing the
running `uv_loop_t*`. In the context of this callback, it is safe to use
this pointer to do various uv_* API calls contained within the `uv::ll`
module. It is not safe to send the `loop_ptr` param to this callback out
via ports/chans.
"]
unsafe fn interact(hl_loop: high_level_loop,
                   -cb: fn~(*c_void)) {
    send_high_level_msg(hl_loop, interaction(cb));
}

fn exit(hl_loop: high_level_loop) unsafe {
    send_high_level_msg(hl_loop, teardown_loop);
}

// INTERNAL API

// data that lives for the lifetime of the high-evel oo
type hl_loop_data = {
    async_handle: *ll::uv_async_t,
    mut active: bool,
    msg_po_ptr: *port<high_level_msg>
};

unsafe fn send_high_level_msg(hl_loop: high_level_loop,
                              -msg: high_level_msg) {
    comm::send(hl_loop.op_chan, msg);
    ll::async_send(hl_loop.async_handle);
}

// this will be invoked by a call to uv::hl::interact() with
// the high_level_loop corresponding to this async_handle. We
// simply check if the loop is active and, if so, invoke the
// user-supplied on_wake callback that is stored in the loop's
// data member
crust fn high_level_wake_up_cb(async_handle: *ll::uv_async_t,
                               status: int) unsafe {
    log(debug, #fmt("high_level_wake_up_cb crust.. handle: %? status: %?",
                     async_handle, status));
    let loop_ptr = ll::get_loop_for_uv_handle(async_handle);
    let data = ll::get_data_for_uv_handle(async_handle) as *hl_loop_data;
    // FIXME: What is this checking?
    if (*data).active {
        let msg_po = *((*data).msg_po_ptr);
        while msg_po.peek() {
            let msg = msg_po.recv();
            if (*data).active {
                alt msg {
                  interaction(cb) {
                    cb(loop_ptr);
                  }
                  teardown_loop {
                    begin_teardown(data);
                  }
                }
            } else {
                // FIXME: drop msg ?
            }
        }
    } else {
        // loop not active
    }
}

crust fn tear_down_close_cb(handle: *ll::uv_async_t) unsafe {
    let loop_ptr = ll::get_loop_for_uv_handle(handle);
    let loop_refs = ll::loop_refcount(loop_ptr);
    log(debug, #fmt("tear_down_close_cb called, closing handle at %? refs %?",
                    handle, loop_refs));
    assert loop_refs == 1i32;
}

fn begin_teardown(data: *hl_loop_data) unsafe {
    log(debug, "high_level_tear_down() called, close async_handle");
    // call user-suppled before_tear_down cb
    let async_handle = (*data).async_handle;
    ll::close(async_handle as *c_void, tear_down_close_cb);
}

#[cfg(test)]
mod test {
    crust fn async_close_cb(handle: *ll::uv_async_t) unsafe {
        log(debug, #fmt("async_close_cb handle %?", handle));
        let exit_ch = (*(ll::get_data_for_uv_handle(handle)
                        as *ah_data)).exit_ch;
        comm::send(exit_ch, ());
    }
    crust fn async_handle_cb(handle: *ll::uv_async_t, status: libc::c_int)
        unsafe {
        log(debug, #fmt("async_handle_cb handle %? status %?",handle,status));
        ll::close(handle, async_close_cb);
    }
    type ah_data = {
        hl_loop: high_level_loop,
        exit_ch: comm::chan<()>
    };
    fn impl_uv_hl_async(hl_loop: high_level_loop) unsafe {
        let async_handle = ll::async_t();
        let ah_ptr = ptr::addr_of(async_handle);
        let exit_po = comm::port::<()>();
        let exit_ch = comm::chan(exit_po);
        let ah_data = {
            hl_loop: hl_loop,
            exit_ch: exit_ch
        };
        let ah_data_ptr = ptr::addr_of(ah_data);
        interact(hl_loop) {|loop_ptr|
            ll::async_init(loop_ptr, ah_ptr, async_handle_cb);
            ll::set_data_for_uv_handle(ah_ptr, ah_data_ptr as *libc::c_void);
            ll::async_send(ah_ptr);
        };
        comm::recv(exit_po);
    }

    // this fn documents the bear minimum neccesary to roll your own
    // high_level_loop
    unsafe fn spawn_test_loop(exit_ch: comm::chan<()>) -> high_level_loop {
        let hl_loop_port = comm::port::<high_level_loop>();
        let hl_loop_ch = comm::chan(hl_loop_port);
        task::spawn_sched(task::manual_threads(1u)) {||
            run_high_level_loop(hl_loop_ch);
            exit_ch.send(());
        };
        ret comm::recv(hl_loop_port);
    }

    crust fn lifetime_handle_close(handle: *libc::c_void) unsafe {
        log(debug, #fmt("lifetime_handle_close ptr %?", handle));
    }

    crust fn lifetime_async_callback(handle: *libc::c_void,
                                     status: libc::c_int) {
        log(debug, #fmt("lifetime_handle_close ptr %? status %?",
                        handle, status));
    }

    #[test]
    fn test_uv_hl_async() unsafe {
        let exit_po = comm::port::<()>();
        let exit_ch = comm::chan(exit_po);
        let hl_loop = spawn_test_loop(exit_ch);

        // using this handle to manage the lifetime of the high_level_loop,
        // as it will exit the first time one of the impl_uv_hl_async() is
        // cleaned up with no one ref'd handles on the loop (Which can happen
        // under race-condition type situations.. this ensures that the loop
        // lives until, at least, all of the impl_uv_hl_async() runs have been
        // called, at least.
        let work_exit_po = comm::port::<()>();
        let work_exit_ch = comm::chan(work_exit_po);
        iter::repeat(7u) {||
            task::spawn_sched(task::manual_threads(1u), {||
                impl_uv_hl_async(hl_loop);
                comm::send(work_exit_ch, ());
            });
        };
        iter::repeat(7u) {||
            comm::recv(work_exit_po);
        };
        log(debug, "sending teardown_loop msg..");
        // the teardown msg usually comes, in the case of the global loop,
        // as a result of receiving a msg on the weaken_task port. but,
        // anyone rolling their own high_level_loop can decide when to
        // send the msg. it's assert and barf, though, if all of your
        // handles aren't uv_close'd first
        comm::send(hl_loop.op_chan, teardown_loop);
        ll::async_send(hl_loop.async_handle);
        comm::recv(exit_po);
        log(debug, "after recv on exit_po.. exiting..");
    }
}
