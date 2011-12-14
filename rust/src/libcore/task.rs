/*
Module: task

Task management.

An executing Rust program consists of a tree of tasks, each with their own
stack, and sole ownership of their allocated heap data. Tasks communicate
with each other using ports and channels.

When a task fails, that failure will propagate to its parent (the task
that spawned it) and the parent will fail as well. The reverse is not
true: when a parent task fails its children will continue executing. When
the root (main) task fails, all tasks fail, and then so does the entire
process.

A task may remove itself from this failure propagation mechanism by
calling the <unsupervise> function, after which failure will only
result in the termination of that task.

Tasks may execute in parallel and are scheduled automatically by the runtime.

Example:

> spawn("Hello, World", fn (&&msg: str) {
>   log msg;
> });

*/
import cast = unsafe::reinterpret_cast;
import comm;
import option::{some, none};
import option = option::t;
import ptr;

export task;
export joinable_task;
export sleep;
export yield;
export task_notification;
export join;
export unsupervise;
export pin;
export unpin;
export set_min_stack;
export task_result;
export tr_success;
export tr_failure;
export get_task;
export spawn;
export spawn_notify;
export spawn_joinable;

#[abi = "rust-intrinsic"]
native mod rusti {
    // these must run on the Rust stack so that they can swap stacks etc:
    fn task_sleep(task: *rust_task, time_in_us: uint, &killed: bool);
}

#[link_name = "rustrt"]
#[abi = "cdecl"]
native mod rustrt {
    // these can run on the C stack:
    fn pin_task();
    fn unpin_task();
    fn get_task_id() -> task_id;
    fn rust_get_task() -> *rust_task;

    fn set_min_stack(stack_size: uint);

    fn new_task() -> task_id;
    fn drop_task(task_id: *rust_task);
    fn get_task_pointer(id: task_id) -> *rust_task;

    fn migrate_alloc(alloc: *u8, target: task_id);

    fn start_task(id: task, closure: *u8);

}

/* Section: Types */

type rust_task =
    {id: task,
     mutable notify_enabled: int,
     mutable notify_chan: comm::chan<task_notification>,
     mutable stack_ptr: *u8};

resource rust_task_ptr(task: *rust_task) { rustrt::drop_task(task); }

type task_id = int;

/*
Type: task

A handle to a task
*/
type task = task_id;

/*
Type: joinable_task

A task that sends notification upon termination
*/
type joinable_task = (task, comm::port<task_notification>);

/*
Tag: task_result

Indicates the manner in which a task exited
*/
tag task_result {
    /* Variant: tr_success */
    tr_success;
    /* Variant: tr_failure */
    tr_failure;
}

/*
Tag: task_notification

Message sent upon task exit to indicate normal or abnormal termination
*/
tag task_notification {
    /* Variant: exit */
    exit(task, task_result);
}

/* Section: Operations */

/*
Type: get_task

Retreives a handle to the currently executing task
*/
fn get_task() -> task { rustrt::get_task_id() }

/*
Function: sleep

Hints the scheduler to yield this task for a specified ammount of time.

Parameters:

time_in_us - maximum number of microseconds to yield control for
*/
fn sleep(time_in_us: uint) {
    let task = rustrt::rust_get_task();
    let killed = false;
    // FIXME: uncomment this when extfmt is moved to core
    // in a snapshot.
    // log #fmt("yielding for %u us", time_in_us);
    rusti::task_sleep(task, time_in_us, killed);
    if killed {
        fail "killed";
    }
}

/*
Function: yield

Yield control to the task scheduler

The scheduler may schedule another task to execute.
*/
fn yield() { sleep(1u) }

/*
Function: join

Wait for a child task to exit

The child task must have been spawned with <spawn_joinable>, which
produces a notification port that the child uses to communicate its
exit status.

Returns:

A task_result indicating whether the task terminated normally or failed
*/
fn join(task_port: joinable_task) -> task_result {
    let (id, port) = task_port;
    alt comm::recv::<task_notification>(port) {
      exit(_id, res) {
        if _id == id {
            ret res
        } else {
            // FIXME: uncomment this when extfmt is moved to core
            // in a snapshot.
            // fail #fmt["join received id %d, expected %d", _id, id]
            fail;
        }
      }
    }
}

/*
Function: unsupervise

Detaches this task from its parent in the task tree

An unsupervised task will not propagate its failure up the task tree
*/
fn unsupervise() { ret sys::unsupervise(); }

/*
Function: pin

Pins the current task and future child tasks to a single scheduler thread
*/
fn pin() { rustrt::pin_task(); }

/*
Function: unpin

Unpin the current task and future child tasks
*/
fn unpin() { rustrt::unpin_task(); }

/*
Function: set_min_stack

Set the minimum stack size (in bytes) for tasks spawned in the future.

This function has global effect and should probably not be used.
*/
fn set_min_stack(stack_size: uint) { rustrt::set_min_stack(stack_size); }

/*
Function: spawn

Creates and executes a new child task

Sets up a new task with its own call stack and schedules it to be executed.
Upon execution the new task will call function `f` with the provided
argument `data`.

Function `f` is a bare function, meaning it may not close over any data, as do
shared functions (fn@) and lambda blocks. `data` must be a uniquely owned
type; it is moved into the new task and thus can no longer be accessed
locally.

Parameters:

data - A unique-type value to pass to the new task
f - A function to execute in the new task

Returns:

A handle to the new task
*/
fn spawn<send T>(-data: T, f: fn(T)) -> task {
    spawn_inner(data, f, none)
}

/*
Function: spawn_notify

Create and execute a new child task, requesting notification upon its
termination

Immediately before termination, either on success or failure, the spawned
task will send a <task_notification> message on the provided channel.
*/
fn spawn_notify<send T>(-data: T, f: fn(T),
                         notify: comm::chan<task_notification>) -> task {
    spawn_inner(data, f, some(notify))
}

/*
Function: spawn_joinable

Create and execute a task which can later be joined with the <join> function

This is a convenience wrapper around spawn_notify which, when paired
with <join> can be easily used to spawn a task then wait for it to
complete.
*/
fn spawn_joinable<send T>(-data: T, f: fn(T)) -> joinable_task {
    let p = comm::port::<task_notification>();
    let id = spawn_notify(data, f, comm::chan::<task_notification>(p));
    ret (id, p);
}

// FIXME: To transition from the unsafe spawn that spawns a shared closure to
// the safe spawn that spawns a bare function we're going to write
// barefunc-spawn on top of unsafe-spawn.  Sadly, bind does not work reliably
// enough to suite our needs (#1034, probably others yet to be discovered), so
// we're going to copy the bootstrap data into a unique pointer, cast it to an
// unsafe pointer then wrap up the bare function and the unsafe pointer in a
// shared closure to spawn.
//
// After the transition this should all be rewritten.

fn spawn_inner<send T>(-data: T, f: fn(T),
                          notify: option<comm::chan<task_notification>>)
    -> task unsafe {

    fn wrapper<send T>(data: *u8, f: fn(T)) unsafe {
        let data: ~T = unsafe::reinterpret_cast(data);
        f(*data);
    }

    let data = ~data;
    let dataptr: *u8 = unsafe::reinterpret_cast(data);
    unsafe::leak(data);
    let wrapped = bind wrapper(dataptr, f);
    ret unsafe_spawn_inner(wrapped, notify);
}

// FIXME: This is the old spawn function that spawns a shared closure.
// It is a hack and needs to be rewritten.
fn unsafe_spawn_inner(-thunk: fn@(),
                      notify: option<comm::chan<task_notification>>) ->
   task unsafe {
    let id = rustrt::new_task();

    let raw_thunk: {code: uint, env: uint} = cast(thunk);

    // set up the task pointer
    let task_ptr <- rust_task_ptr(rustrt::get_task_pointer(id));

    assert (ptr::null() != (**task_ptr).stack_ptr);

    // copy the thunk from our stack to the new stack
    let sp: uint = cast((**task_ptr).stack_ptr);
    let ptrsize = sys::size_of::<*u8>();
    let thunkfn: *mutable uint = cast(sp - ptrsize * 2u);
    let thunkenv: *mutable uint = cast(sp - ptrsize);
    *thunkfn = cast(raw_thunk.code);;
    *thunkenv = cast(raw_thunk.env);;
    // align the stack to 16 bytes
    (**task_ptr).stack_ptr = cast(sp - ptrsize * 4u);

    // set up notifications if they are enabled.
    alt notify {
      some(c) {
        (**task_ptr).notify_enabled = 1;
        (**task_ptr).notify_chan = c;
      }
      none { }
    }

    // give the thunk environment's allocation to the new task
    rustrt::migrate_alloc(cast(raw_thunk.env), id);
    rustrt::start_task(id, cast(thunkfn));
    // don't cleanup the thunk in this task
    unsafe::leak(thunk);
    ret id;
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
