#[doc = "

A task-based interface to the uv loop

The I/O task runs in its own single-threaded scheduler.  By using the
`interact` function you can execute code in a uv callback.

"];

export iotask::{};
export spawn_iotask;
export interact;
export exit;

import libc::c_void;
import ptr::addr_of;
import comm::{port, chan, methods};
import ll = uv_ll;

#[doc = "
Used to abstract-away direct interaction with a libuv loop.
"]
enum iotask {
    iotask_({
        async_handle: *ll::uv_async_t,
        op_chan: chan<iotask_msg>
    })
}

fn spawn_iotask(-builder: task::builder) -> iotask {

    import task::{set_opts, get_opts, single_threaded, run};

    let iotask_po = port::<iotask>();
    let iotask_ch = iotask_po.chan();

    set_opts(builder, {
        sched: some({
            mode: single_threaded,
            native_stack_size: none
        })
        with get_opts(builder)
    });

    run(builder) {||
        #debug("entering libuv task");
        run_loop(iotask_ch);
        #debug("libuv task exiting");
    };

    iotask_po.recv()
}


#[doc = "
Provide a callback to be processed by `iotask`

The primary way to do operations again a running `iotask` that
doesn't involve creating a uv handle via `safe_handle`

# Warning

This function is the only safe way to interact with _any_ `iotask`.
Using functions in the `uv::ll` module outside of the `cb` passed into
this function is _very dangerous_.

# Arguments

* iotask - a uv I/O task that you want to do operations against
* cb - a function callback to be processed on the running loop's
thread. The only parameter passed in is an opaque pointer representing the
running `uv_loop_t*`. In the context of this callback, it is safe to use
this pointer to do various uv_* API calls contained within the `uv::ll`
module. It is not safe to send the `loop_ptr` param to this callback out
via ports/chans.
"]
unsafe fn interact(iotask: iotask,
                   -cb: fn~(*c_void)) {
    send_msg(iotask, interaction(cb));
}

#[doc="
Shut down the I/O task

Is used to signal to the loop that it should close the internally-held
async handle and do a sanity check to make sure that all other handles are
closed, causing a failure otherwise.
"]
fn exit(iotask: iotask) unsafe {
    send_msg(iotask, teardown_loop);
}


// INTERNAL API

enum iotask_msg {
    interaction (fn~(*libc::c_void)),
    teardown_loop
}

#[doc = "
Run the loop and begin handling messages
"]
fn run_loop(iotask_ch: chan<iotask>) unsafe {
    let msg_po = port::<iotask_msg>();
    let loop_ptr = ll::loop_new();
    // set up the special async handle we'll use to allow multi-task
    // communication with this loop
    let async = ll::async_t();
    let async_handle = addr_of(async);
    // associate the async handle with the loop
    ll::async_init(loop_ptr, async_handle, wake_up_cb);

    // initialize our loop data and store it in the loop
    let data: iotask_loop_data = {
        async_handle: async_handle,
        msg_po_ptr: addr_of(msg_po)
    };
    ll::set_data_for_uv_handle(async_handle, addr_of(data));

    // Send out a handle through which folks can talk to us
    // while we dwell in the I/O loop
    let iotask = iotask_({
        async_handle: async_handle,
        op_chan: msg_po.chan()
    });
    iotask_ch.send(iotask);

    log(debug, "about to run uv loop");
    // enter the loop... this blocks until the loop is done..
    ll::run(loop_ptr);
    log(debug, "uv loop ended");
    ll::loop_delete(loop_ptr);
}

// data that lives for the lifetime of the high-evel oo
type iotask_loop_data = {
    async_handle: *ll::uv_async_t,
    msg_po_ptr: *port<iotask_msg>
};

fn send_msg(iotask: iotask,
            -msg: iotask_msg) unsafe {
    iotask.op_chan.send(msg);
    ll::async_send(iotask.async_handle);
}

// this will be invoked by a call to uv::hl::interact() with
// the high_level_loop corresponding to this async_handle. We
// simply check if the loop is active and, if so, invoke the
// user-supplied on_wake callback that is stored in the loop's
// data member
crust fn wake_up_cb(async_handle: *ll::uv_async_t,
                    status: int) unsafe {
    log(debug, #fmt("wake_up_cb crust.. handle: %? status: %?",
                     async_handle, status));
    let loop_ptr = ll::get_loop_for_uv_handle(async_handle);
    let data = ll::get_data_for_uv_handle(async_handle) as *iotask_loop_data;
    let msg_po = *((*data).msg_po_ptr);
    while msg_po.peek() {
        let msg = msg_po.recv();
        alt msg {
          interaction(cb) {
            cb(loop_ptr);
          }
          teardown_loop {
            begin_teardown(data);
          }
        }
    }
}

fn begin_teardown(data: *iotask_loop_data) unsafe {
    log(debug, "iotask begin_teardown() called, close async_handle");
    // call user-suppled before_tear_down cb
    let async_handle = (*data).async_handle;
    ll::close(async_handle as *c_void, tear_down_close_cb);
}

crust fn tear_down_close_cb(handle: *ll::uv_async_t) unsafe {
    let loop_ptr = ll::get_loop_for_uv_handle(handle);
    let loop_refs = ll::loop_refcount(loop_ptr);
    log(debug, #fmt("tear_down_close_cb called, closing handle at %? refs %?",
                    handle, loop_refs));
    assert loop_refs == 1i32;
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
        iotask: iotask,
        exit_ch: comm::chan<()>
    };
    fn impl_uv_iotask_async(iotask: iotask) unsafe {
        let async_handle = ll::async_t();
        let ah_ptr = ptr::addr_of(async_handle);
        let exit_po = comm::port::<()>();
        let exit_ch = comm::chan(exit_po);
        let ah_data = {
            iotask: iotask,
            exit_ch: exit_ch
        };
        let ah_data_ptr = ptr::addr_of(ah_data);
        interact(iotask) {|loop_ptr|
            ll::async_init(loop_ptr, ah_ptr, async_handle_cb);
            ll::set_data_for_uv_handle(ah_ptr, ah_data_ptr as *libc::c_void);
            ll::async_send(ah_ptr);
        };
        comm::recv(exit_po);
    }

    // this fn documents the bear minimum neccesary to roll your own
    // high_level_loop
    unsafe fn spawn_test_loop(exit_ch: comm::chan<()>) -> iotask {
        let iotask_port = comm::port::<iotask>();
        let iotask_ch = comm::chan(iotask_port);
        task::spawn_sched(task::manual_threads(1u)) {||
            run_loop(iotask_ch);
            exit_ch.send(());
        };
        ret comm::recv(iotask_port);
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
    fn test_uv_iotask_async() unsafe {
        let exit_po = comm::port::<()>();
        let exit_ch = comm::chan(exit_po);
        let iotask = spawn_test_loop(exit_ch);

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
                impl_uv_iotask_async(iotask);
                comm::send(work_exit_ch, ());
            });
        };
        iter::repeat(7u) {||
            comm::recv(work_exit_po);
        };
        log(debug, "sending teardown_loop msg..");
        exit(iotask);
        comm::recv(exit_po);
        log(debug, "after recv on exit_po.. exiting..");
    }
}
