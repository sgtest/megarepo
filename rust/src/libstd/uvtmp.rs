// Some temporary libuv hacks for servo

// UV2

// these are processed solely in the
// process_operation() crust fn below
enum uv_operation {
    op_async_init([u8]),
    op_close(uv_handle, *ctypes::void)
}

enum uv_handle {
    uv_async([u8], uv_loop)
}

enum uv_msg {
    // requests from library users
    msg_run(comm::chan<bool>),
    msg_run_in_bg(),
    msg_async_init(fn~(uv_handle), fn~(uv_handle)),
    msg_async_send([u8]),
    msg_close(uv_handle, fn~()),

    // dispatches from libuv
    uv_async_init([u8], *ctypes::void),
    uv_async_send([u8]),
    uv_close([u8]),
    uv_end()
}

type uv_loop_data = {
    operation_port: comm::port<uv_operation>,
    rust_loop_chan: comm::chan<uv_msg>
};

type uv_loop = comm::chan<uv_msg>;

#[nolink]
native mod rustrt {
    fn rust_uvtmp_create_thread() -> thread;
    fn rust_uvtmp_start_thread(thread: thread);
    fn rust_uvtmp_join_thread(thread: thread);
    fn rust_uvtmp_delete_thread(thread: thread);
    fn rust_uvtmp_connect(
        thread: thread,
        req_id: u32,
        ip: str::sbuf,
        chan: comm::chan<iomsg>) -> connect_data;
    fn rust_uvtmp_close_connection(thread: thread, req_id: u32);
    fn rust_uvtmp_write(
        thread: thread,
        req_id: u32,
        buf: *u8,
        len: ctypes::size_t,
        chan: comm::chan<iomsg>);
    fn rust_uvtmp_read_start(
        thread: thread,
        req_id: u32,
        chan: comm::chan<iomsg>);
    fn rust_uvtmp_timer(
        thread: thread,
        timeout: u32,
        req_id: u32,
        chan: comm::chan<iomsg>);
    fn rust_uvtmp_delete_buf(buf: *u8);
    fn rust_uvtmp_get_req_id(cd: connect_data) -> u32;

    fn rust_uvtmp_uv_loop_new() -> *ctypes::void;
    fn rust_uvtmp_uv_loop_set_data(
        loop: *ctypes::void,
        data: *uv_loop_data);
    fn rust_uvtmp_uv_bind_op_cb(loop: *ctypes::void, cb: *u8) -> *ctypes::void;
    fn rust_uvtmp_uv_stop_op_cb(handle: *ctypes::void);
    fn rust_uvtmp_uv_run(loop_handle: *ctypes::void);
    fn rust_uvtmp_uv_close(handle: *ctypes::void, cb: *u8);
    fn rust_uvtmp_uv_close_async(handle: *ctypes::void);
    fn rust_uvtmp_uv_async_send(handle: *ctypes::void);
    fn rust_uvtmp_uv_async_init(
        loop_handle: *ctypes::void,
        cb: *u8,
        id: *u8) -> *ctypes::void;
}

mod uv {
    export loop_new, run, close, run_in_bg, async_init, async_send,
           timer_init;

    // public functions
    fn loop_new() -> uv_loop unsafe {
        let ret_recv_port: comm::port<uv_loop> =
            comm::port();
        let ret_recv_chan: comm::chan<uv_loop> =
            comm::chan(ret_recv_port);

        let num_threads = 4u; // would be cool to tie this to
                              // the number of logical procs
        task::spawn_sched(num_threads) {||
            // our beloved uv_loop_t ptr
            let loop_handle = rustrt::
                rust_uvtmp_uv_loop_new();

            // this port/chan pair are used to send messages to
            // libuv. libuv processes any pending messages on the
            // port (via crust) after receiving an async "wakeup"
            // on a special uv_async_t handle created below
            let operation_port = comm::port::<uv_operation>();
            let operation_chan = comm::chan::<uv_operation>(
                operation_port);

            // this port/chan pair as used in the while() loop
            // below. It takes dispatches, originating from libuv
            // callbacks, to invoke handles registered by the
            // user
            let rust_loop_port = comm::port::<uv_msg>();
            let rust_loop_chan =
                comm::chan::<uv_msg>(rust_loop_port);
            // let the task-spawner return
            comm::send(ret_recv_chan, copy(rust_loop_chan));

            // create our "special" async handle that will
            // allow all operations against libuv to be
            // "buffered" in the operation_port, for processing
            // from the thread that libuv runs on
            let loop_data: uv_loop_data = {
                operation_port: operation_port,
                rust_loop_chan: rust_loop_chan
            };
            rustrt::rust_uvtmp_uv_loop_set_data(
                loop_handle,
                ptr::addr_of(loop_data)); // pass an opaque C-ptr
                                          // to libuv, this will be
                                          // in the process_operation
                                          // crust fn
            let op_handle = rustrt::rust_uvtmp_uv_bind_op_cb(
                loop_handle,
                process_operation);

            // all state goes here
            let handles: map::map<[u8], *ctypes::void> =
                map::new_bytes_hash();
            let id_to_handle: map::map<[u8], uv_handle> =
                map::new_bytes_hash();
            let async_cbs: map::map<[u8], fn~(uv_handle)> =
                map::new_bytes_hash();
            let async_init_after_cbs: map::map<[u8],
                                               fn~(uv_handle)> =
                map::new_bytes_hash();
            let close_callbacks: map::map<[u8], fn~()> =
                map::new_bytes_hash();

            // the main loop that this task blocks on.
            // should have the same lifetime as the C libuv
            // event loop.
            let keep_going = true;
            while (keep_going) {
                alt comm::recv(rust_loop_port) {
                  msg_run(end_chan) {
                    // start the libuv event loop
                    // we'll also do a uv_async_send with
                    // the operation handle to have the
                    // loop process any pending operations
                    // once its up and running
                    task::spawn_sched(1u) {||
                        // this call blocks
                        rustrt::rust_uvtmp_uv_run(loop_handle);
                        // when we're done, msg the
                        // end chan
                        rustrt::rust_uvtmp_uv_stop_op_cb(op_handle);
                        comm::send(end_chan, true);
                        comm::send(rust_loop_chan, uv_end);
                    };
                  }
                  
                  msg_run_in_bg {
                    task::spawn_sched(1u) {||
                        // this call blocks
                        rustrt::rust_uvtmp_uv_run(loop_handle);
                    };
                  }
                  
                  msg_close(handle, cb) {
                    let id = get_id_from_handle(handle);
                    close_callbacks.insert(id, cb);
                    let handle_ptr = handles.get(id);
                    let op = op_close(handle, handle_ptr);

                    pass_to_libuv(op_handle, operation_chan, op);
                  }
                  uv_close(id) {
                    handles.remove(id);
                    let handle = id_to_handle.get(id);
                    id_to_handle.remove(id);
                    alt handle {
                      uv_async(id, _) {
                        async_cbs.remove(id);
                      }
                      _ {
                        fail "unknown form of uv_handle encountered "
                            + "in uv_close handler";
                      }
                    }
                    let cb = close_callbacks.get(id);
                    close_callbacks.remove(id);
                    task::spawn {||
                        cb();
                    };
                  }
                  
                  msg_async_init(callback, after_cb) {
                    // create a new async handle
                    // with the id as the handle's
                    // data and save the callback for
                    // invocation on msg_async_send
                    let id = gen_handle_id();
                    async_cbs.insert(id, callback);
                    async_init_after_cbs.insert(id, after_cb);
                    let op = op_async_init(id);
                    pass_to_libuv(op_handle, operation_chan, op);
                  }
                  uv_async_init(id, async_handle) {
                    // libuv created a handle, which is
                    // passed back to us. save it and
                    // then invoke the supplied callback
                    // for after completion
                    handles.insert(id, async_handle);
                    let after_cb = async_init_after_cbs.get(id);
                    async_init_after_cbs.remove(id);
                    let async = uv_async(id, rust_loop_chan);
                    id_to_handle.insert(id, copy(async));
                    task::spawn {||
                        after_cb(async);
                    };
                  }

                  msg_async_send(id) {
                    let async_handle = handles.get(id);
                    do_send(async_handle);
                  }
                  uv_async_send(id) {
                    let async_cb = async_cbs.get(id);
                    task::spawn {||
                        async_cb(uv_async(id, rust_loop_chan));
                    };
                  }
                  uv_end() {
                    keep_going = false;
                  }

                  _ { fail "unknown form of uv_msg received"; }
                }
            }
        };
        ret comm::recv(ret_recv_port);
    }

    fn run(loop: uv_loop) {
        let end_port = comm::port::<bool>();
        let end_chan = comm::chan::<bool>(end_port);
        comm::send(loop, msg_run(end_chan));
        comm::recv(end_port);
    }

    fn run_in_bg(loop: uv_loop) {
        comm::send(loop, msg_run_in_bg);
    }

    fn async_init (
        loop: uv_loop,
        async_cb: fn~(uv_handle),
        after_cb: fn~(uv_handle)) {
        let msg = msg_async_init(async_cb, after_cb);
        comm::send(loop, msg);
    }

    fn async_send(async: uv_handle) {
        alt async {
          uv_async(id, loop) {
            comm::send(loop, msg_async_send(id));
          }
          _ {
            fail "attempting to call async_send() with a" +
                " uv_async uv_handle";
          }
        }
    }

    fn close(h: uv_handle, cb: fn~()) {
        let loop_chan = get_loop_chan_from_handle(h);
        comm::send(loop_chan, msg_close(h, cb)); 
    }

    fn timer_init(loop: uv_loop, after_cb: fn~(uv_handle)) {
        let msg = msg_timer_init(after_cb);
        comm::send(loop, msg);
    }

    // internal functions
    fn pass_to_libuv(
            op_handle: *ctypes::void,
            operation_chan: comm::chan<uv_operation>,
            op: uv_operation) unsafe {
        comm::send(operation_chan, copy(op));
        do_send(op_handle);
    }
    fn do_send(h: *ctypes::void) {
        rustrt::rust_uvtmp_uv_async_send(h);
    }
    fn gen_handle_id() -> [u8] {
        ret rand::mk_rng().gen_bytes(16u);
    }
    fn get_handle_id_from(buf: *u8) -> [u8] unsafe {
        ret vec::unsafe::from_buf(buf, 16u); 
    }

    fn get_loop_chan_from_data(data: *uv_loop_data)
            -> uv_loop unsafe {
        ret (*data).rust_loop_chan;
    }

    fn get_loop_chan_from_handle(handle: uv_handle)
        -> uv_loop {
        alt handle {
          uv_async(id,loop) {
            ret loop;
          }
          _ {
            fail "unknown form of uv_handle for get_loop_chan_from "
                 + " handle";
          }
        }
    }

    fn get_id_from_handle(handle: uv_handle) -> [u8] {
        alt handle {
          uv_async(id,loop) {
            ret id;
          }
          _ {
            fail "unknown form of uv_handle for get_id_from handle";
          }
        }
    }

    // crust
    crust fn process_operation(
            loop: *ctypes::void,
            data: *uv_loop_data) unsafe {
        let op_port = (*data).operation_port;
        let loop_chan = get_loop_chan_from_data(data);
        let op_pending = comm::peek(op_port);
        while(op_pending) {
            alt comm::recv(op_port) {
              op_async_init(id) {
                let id_ptr = vec::unsafe::to_ptr(id);
                let async_handle = rustrt::rust_uvtmp_uv_async_init(
                    loop,
                    process_async_send,
                    id_ptr);
                comm::send(loop_chan, uv_async_init(
                    id,
                    async_handle));
              }
              op_close(handle, handle_ptr) {
                handle_op_close(handle, handle_ptr);
              }
              
              _ { fail "unknown form of uv_operation received"; }
            }
            op_pending = comm::peek(op_port);
        }
    }

    fn handle_op_close(handle: uv_handle, handle_ptr: *ctypes::void) {
        // it's just like im doing C
        alt handle {
          uv_async(id, loop) {
            let cb = process_close_async;
            rustrt::rust_uvtmp_uv_close(
                handle_ptr, cb);
          }
          _ {
            fail "unknown form of uv_handle encountered " +
                "in process_operation/op_close";
          }
        }
    }

    crust fn process_async_send(id_buf: *u8, data: *uv_loop_data)
            unsafe {
        let handle_id = get_handle_id_from(id_buf);
        let loop_chan = get_loop_chan_from_data(data);
        comm::send(loop_chan, uv_async_send(handle_id));
    }

    fn process_close_common(id: [u8], data: *uv_loop_data)
        unsafe {
        // notify the rust loop that their handle is closed, then
        // the caller will invoke a per-handle-type c++ func to
        // free allocated memory
        let loop_chan = get_loop_chan_from_data(data);
        comm::send(loop_chan, uv_close(id));
    }

    crust fn process_close_async(
        id_buf: *u8,
        handle_ptr: *ctypes::void,
        data: *uv_loop_data)
        unsafe {
        let id = get_handle_id_from(id_buf);
        rustrt::rust_uvtmp_uv_close_async(handle_ptr);
        // at this point, the handle and its data has been
        // released. notify the rust loop to remove the
        // handle and its data and call the user-supplied
        // close cb
        process_close_common(id, data);
    }

    
}

#[test]
fn test_uvtmp_uv_new_loop_no_handles() {
    let test_loop = uv::loop_new();
    uv::run(test_loop); // this should return immediately
                        // since there aren't any handles..
}

#[test]
fn test_uvtmp_uv_simple_async() {
    let test_loop = uv::loop_new();
    let exit_port = comm::port::<bool>();
    let exit_chan = comm::chan::<bool>(exit_port);
    uv::async_init(test_loop, {|new_async|
        uv::close(new_async) {||
            comm::send(exit_chan, true);
        };
    }, {|new_async|
        uv::async_send(new_async);
    });
    uv::run(test_loop);
    assert comm::recv(exit_port);
}

#[test]
fn test_uvtmp_uv_timer() {
    let test_loop = uv::loop_new();
    let exit_port = comm::port::<bool>();
    let exit_chan = comm::chan::<bool>(exit_port);
    uv::timer(test_loop, {|new_timer|
        uv::timer_start(new_async) {||
            comm::send(exit_chan, true);
        };
    }); 
    uv::run(test_loop);
    assert comm::recv(exit_port);
}

// END OF UV2

type thread = *ctypes::void;

type connect_data = *ctypes::void;

enum iomsg {
    whatever,
    connected(connect_data),
    wrote(connect_data),
    read(connect_data, *u8, ctypes::ssize_t),
    timer(u32),
    exit
}

fn create_thread() -> thread {
    rustrt::rust_uvtmp_create_thread()
}

fn start_thread(thread: thread) {
    rustrt::rust_uvtmp_start_thread(thread)
}

fn join_thread(thread: thread) {
    rustrt::rust_uvtmp_join_thread(thread)
}

fn delete_thread(thread: thread) {
    rustrt::rust_uvtmp_delete_thread(thread)
}

fn connect(thread: thread, req_id: u32,
           ip: str, ch: comm::chan<iomsg>) -> connect_data {
    str::as_buf(ip) {|ipbuf|
        rustrt::rust_uvtmp_connect(thread, req_id, ipbuf, ch)
    }
}

fn close_connection(thread: thread, req_id: u32) {
    rustrt::rust_uvtmp_close_connection(thread, req_id);
}

fn write(thread: thread, req_id: u32, bytes: [u8],
         chan: comm::chan<iomsg>) unsafe {
    rustrt::rust_uvtmp_write(
        thread, req_id, vec::to_ptr(bytes), vec::len(bytes), chan);
}

fn read_start(thread: thread, req_id: u32,
              chan: comm::chan<iomsg>) {
    rustrt::rust_uvtmp_read_start(thread, req_id, chan);
}

fn timer_start(thread: thread, timeout: u32, req_id: u32,
              chan: comm::chan<iomsg>) {
    rustrt::rust_uvtmp_timer(thread, timeout, req_id, chan);
}

fn delete_buf(buf: *u8) {
    rustrt::rust_uvtmp_delete_buf(buf);
}

fn get_req_id(cd: connect_data) -> u32 {
    ret rustrt::rust_uvtmp_get_req_id(cd);
}

#[test]
fn test_start_stop() {
    let thread = create_thread();
    start_thread(thread);
    join_thread(thread);
    delete_thread(thread);
}

#[test]
#[ignore]
fn test_connect() {
    let thread = create_thread();
    start_thread(thread);
    let port = comm::port();
    let chan = comm::chan(port);
    connect(thread, 0u32, "74.125.224.146", chan);
    alt comm::recv(port) {
      connected(cd) {
        close_connection(thread, 0u32);
      }
      _ { fail "test_connect: port isn't connected"; }
    }
    join_thread(thread);
    delete_thread(thread);
}

#[test]
#[ignore]
fn test_http() {
    let thread = create_thread();
    start_thread(thread);
    let port = comm::port();
    let chan = comm::chan(port);
    connect(thread, 0u32, "74.125.224.146", chan);
    alt comm::recv(port) {
      connected(cd) {
        write(thread, 0u32, str::bytes("GET / HTTP/1.0\n\n"), chan);
        alt comm::recv(port) {
          wrote(cd) {
            read_start(thread, 0u32, chan);
            let keep_going = true;
            while keep_going {
                alt comm::recv(port) {
                  read(_, buf, -1) {
                    keep_going = false;
                    delete_buf(buf);
                  }
                  read(_, buf, len) {
                    unsafe {
                        log(error, len);
                        let buf = vec::unsafe::from_buf(buf,
                                                        len as uint);
                        let str = str::from_bytes(buf);
                        #error("read something");
                        io::println(str);
                    }
                    delete_buf(buf);
                  }
                  _ { fail "test_http: protocol error"; }
                }
            }
            close_connection(thread, 0u32);
          }
          _ { fail "test_http: expected `wrote`"; }
        }
      }
      _ { fail "test_http: port not connected"; }
    }
    join_thread(thread);
    delete_thread(thread);
}
