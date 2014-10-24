// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The Green Task implementation
//!
//! This module contains the glue to the libstd runtime necessary to integrate
//! M:N scheduling. This GreenTask structure is hidden as a trait object in all
//! rust tasks and virtual calls are made in order to interface with it.
//!
//! Each green task contains a scheduler if it is currently running, and it also
//! contains the rust task itself in order to juggle around ownership of the
//! values.

use std::any::Any;
use std::mem;
use std::raw;
use std::rt::Runtime;
use std::rt::local::Local;
use std::rt::mutex::NativeMutex;
use std::rt::rtio;
use std::rt::stack;
use std::rt::task::{Task, BlockedTask, TaskOpts};
use std::rt;

use context::Context;
use coroutine::Coroutine;
use sched::{Scheduler, SchedHandle, RunOnce};
use stack::StackPool;

/// The necessary fields needed to keep track of a green task (as opposed to a
/// 1:1 task).
pub struct GreenTask {
    /// Coroutine that this task is running on, otherwise known as the register
    /// context and the stack that this task owns. This field is optional to
    /// relinquish ownership back to a scheduler to recycle stacks at a later
    /// date.
    pub coroutine: Option<Coroutine>,

    /// Optional handle back into the home sched pool of this task. This field
    /// is lazily initialized.
    pub handle: Option<SchedHandle>,

    /// Slot for maintaining ownership of a scheduler. If a task is running,
    /// this value will be Some(sched) where the task is running on "sched".
    pub sched: Option<Box<Scheduler>>,

    /// Temporary ownership slot of a std::rt::task::Task object. This is used
    /// to squirrel that libstd task away while we're performing green task
    /// operations.
    pub task: Option<Box<Task>>,

    /// Dictates whether this is a sched task or a normal green task
    pub task_type: TaskType,

    /// Home pool that this task was spawned into. This field is lazily
    /// initialized until when the task is initially scheduled, and is used to
    /// make sure that tasks are always woken up in the correct pool of
    /// schedulers.
    pub pool_id: uint,

    // See the comments in the scheduler about why this is necessary
    pub nasty_deschedule_lock: NativeMutex,
}

pub enum TaskType {
    TypeGreen(Option<Home>),
    TypeSched,
}

pub enum Home {
    AnySched,
    HomeSched(SchedHandle),
}

/// Trampoline code for all new green tasks which are running around. This
/// function is passed through to Context::new as the initial rust landing pad
/// for all green tasks. This code is actually called after the initial context
/// switch onto a green thread.
///
/// The first argument to this function is the `Box<GreenTask>` pointer, and
/// the next two arguments are the user-provided procedure for running code.
///
/// The goal for having this weird-looking function is to reduce the number of
/// allocations done on a green-task startup as much as possible.
extern fn bootstrap_green_task(task: uint, code: *mut (), env: *mut ()) -> ! {
    // Acquire ownership of the `proc()`
    let start: proc() = unsafe {
        mem::transmute(raw::Procedure { code: code, env: env })
    };

    // Acquire ownership of the `Box<GreenTask>`
    let mut task: Box<GreenTask> = unsafe { mem::transmute(task) };

    // First code after swap to this new context. Run our cleanup job
    task.pool_id = {
        let sched = task.sched.as_mut().unwrap();
        sched.run_cleanup_job();
        sched.task_state.increment();
        sched.pool_id
    };

    // Convert our green task to a libstd task and then execute the code
    // requested. This is the "try/catch" block for this green task and
    // is the wrapper for *all* code run in the task.
    let mut start = Some(start);
    let task = task.swap().run(|| start.take().unwrap()()).destroy();

    // Once the function has exited, it's time to run the termination
    // routine. This means we need to context switch one more time but
    // clean ourselves up on the other end. Since we have no way of
    // preserving a handle to the GreenTask down to this point, this
    // unfortunately must call `GreenTask::convert`. In order to avoid
    // this we could add a `terminate` function to the `Runtime` trait
    // in libstd, but that seems less appropriate since the conversion
    // method exists.
    GreenTask::convert(task).terminate();
}

impl GreenTask {
    /// Creates a new green task which is not homed to any particular scheduler
    /// and will not have any contained Task structure.
    pub fn new(stack_pool: &mut StackPool,
               stack_size: Option<uint>,
               start: proc():Send) -> Box<GreenTask> {
        GreenTask::new_homed(stack_pool, stack_size, AnySched, start)
    }

    /// Creates a new task (like `new`), but specifies the home for new task.
    pub fn new_homed(stack_pool: &mut StackPool,
                     stack_size: Option<uint>,
                     home: Home,
                     start: proc():Send) -> Box<GreenTask> {
        // Allocate ourselves a GreenTask structure
        let mut ops = GreenTask::new_typed(None, TypeGreen(Some(home)));

        // Allocate a stack for us to run on
        let stack_size = stack_size.unwrap_or_else(|| rt::min_stack());
        let mut stack = stack_pool.take_stack(stack_size);
        let context = Context::new(bootstrap_green_task, ops.as_uint(), start,
                                   &mut stack);

        // Package everything up in a coroutine and return
        ops.coroutine = Some(Coroutine {
            current_stack_segment: stack,
            saved_context: context,
        });
        return ops;
    }

    /// Creates a new green task with the specified coroutine and type, this is
    /// useful when creating scheduler tasks.
    pub fn new_typed(coroutine: Option<Coroutine>,
                     task_type: TaskType) -> Box<GreenTask> {
        box GreenTask {
            pool_id: 0,
            coroutine: coroutine,
            task_type: task_type,
            sched: None,
            handle: None,
            nasty_deschedule_lock: unsafe { NativeMutex::new() },
            task: Some(box Task::new()),
        }
    }

    /// Creates a new green task with the given configuration options for the
    /// contained Task object. The given stack pool is also used to allocate a
    /// new stack for this task.
    pub fn configure(pool: &mut StackPool,
                     opts: TaskOpts,
                     f: proc():Send) -> Box<GreenTask> {
        let TaskOpts { name, stack_size, on_exit } = opts;

        let mut green = GreenTask::new(pool, stack_size, f);
        {
            let task = green.task.as_mut().unwrap();
            task.name = name;
            task.death.on_exit = on_exit;
        }
        return green;
    }

    /// Just like the `maybe_take_runtime` function, this function should *not*
    /// exist. Usage of this function is _strongly_ discouraged. This is an
    /// absolute last resort necessary for converting a libstd task to a green
    /// task.
    ///
    /// This function will assert that the task is indeed a green task before
    /// returning (and will kill the entire process if this is wrong).
    pub fn convert(mut task: Box<Task>) -> Box<GreenTask> {
        match task.maybe_take_runtime::<GreenTask>() {
            Some(mut green) => {
                green.put_task(task);
                green
            }
            None => rtabort!("not a green task any more?"),
        }
    }

    pub fn give_home(&mut self, new_home: Home) {
        match self.task_type {
            TypeGreen(ref mut home) => { *home = Some(new_home); }
            TypeSched => rtabort!("type error: used SchedTask as GreenTask"),
        }
    }

    pub fn take_unwrap_home(&mut self) -> Home {
        match self.task_type {
            TypeGreen(ref mut home) => home.take().unwrap(),
            TypeSched => rtabort!("type error: used SchedTask as GreenTask"),
        }
    }

    // New utility functions for homes.

    pub fn is_home_no_tls(&self, sched: &Scheduler) -> bool {
        match self.task_type {
            TypeGreen(Some(AnySched)) => { false }
            TypeGreen(Some(HomeSched(SchedHandle { sched_id: ref id, .. }))) => {
                *id == sched.sched_id()
            }
            TypeGreen(None) => { rtabort!("task without home"); }
            TypeSched => {
                // Awe yea
                rtabort!("type error: expected: TypeGreen, found: TaskSched");
            }
        }
    }

    pub fn homed(&self) -> bool {
        match self.task_type {
            TypeGreen(Some(AnySched)) => { false }
            TypeGreen(Some(HomeSched(SchedHandle { .. }))) => { true }
            TypeGreen(None) => {
                rtabort!("task without home");
            }
            TypeSched => {
                rtabort!("type error: expected: TypeGreen, found: TaskSched");
            }
        }
    }

    pub fn is_sched(&self) -> bool {
        match self.task_type {
            TypeGreen(..) => false, TypeSched => true,
        }
    }

    // Unsafe functions for transferring ownership of this GreenTask across
    // context switches

    pub fn as_uint(&self) -> uint {
        self as *const GreenTask as uint
    }

    pub unsafe fn from_uint(val: uint) -> Box<GreenTask> {
        mem::transmute(val)
    }

    // Runtime glue functions and helpers

    pub fn put_with_sched(mut self: Box<GreenTask>, sched: Box<Scheduler>) {
        assert!(self.sched.is_none());
        self.sched = Some(sched);
        self.put();
    }

    pub fn put_task(&mut self, task: Box<Task>) {
        assert!(self.task.is_none());
        self.task = Some(task);
    }

    pub fn swap(mut self: Box<GreenTask>) -> Box<Task> {
        let mut task = self.task.take().unwrap();
        task.put_runtime(self);
        return task;
    }

    pub fn put(self: Box<GreenTask>) {
        assert!(self.sched.is_some());
        Local::put(self.swap());
    }

    fn terminate(mut self: Box<GreenTask>) -> ! {
        let sched = self.sched.take().unwrap();
        sched.terminate_current_task(self)
    }

    // This function is used to remotely wakeup this green task back on to its
    // original pool of schedulers. In order to do so, each tasks arranges a
    // SchedHandle upon descheduling to be available for sending itself back to
    // the original pool.
    //
    // Note that there is an interesting transfer of ownership going on here. We
    // must relinquish ownership of the green task, but then also send the task
    // over the handle back to the original scheduler. In order to safely do
    // this, we leverage the already-present "nasty descheduling lock". The
    // reason for doing this is that each task will bounce on this lock after
    // resuming after a context switch. By holding the lock over the enqueueing
    // of the task, we're guaranteed that the SchedHandle's memory will be valid
    // for this entire function.
    //
    // An alternative would include having incredibly cheaply cloneable handles,
    // but right now a SchedHandle is something like 6 allocations, so it is
    // *not* a cheap operation to clone a handle. Until the day comes that we
    // need to optimize this, a lock should do just fine (it's completely
    // uncontended except for when the task is rescheduled).
    fn reawaken_remotely(mut self: Box<GreenTask>) {
        unsafe {
            let mtx = &mut self.nasty_deschedule_lock as *mut NativeMutex;
            let handle = self.handle.as_mut().unwrap() as *mut SchedHandle;
            let _guard = (*mtx).lock();
            (*handle).send(RunOnce(self));
        }
    }
}

impl Runtime for GreenTask {
    fn yield_now(mut self: Box<GreenTask>, cur_task: Box<Task>) {
        self.put_task(cur_task);
        let sched = self.sched.take().unwrap();
        sched.yield_now(self);
    }

    fn maybe_yield(mut self: Box<GreenTask>, cur_task: Box<Task>) {
        self.put_task(cur_task);
        let sched = self.sched.take().unwrap();
        sched.maybe_yield(self);
    }

    fn deschedule(mut self: Box<GreenTask>,
                  times: uint,
                  cur_task: Box<Task>,
                  f: |BlockedTask| -> Result<(), BlockedTask>) {
        self.put_task(cur_task);
        let mut sched = self.sched.take().unwrap();

        // In order for this task to be reawoken in all possible contexts, we
        // may need a handle back in to the current scheduler. When we're woken
        // up in anything other than the local scheduler pool, this handle is
        // used to send this task back into the scheduler pool.
        if self.handle.is_none() {
            self.handle = Some(sched.make_handle());
            self.pool_id = sched.pool_id;
        }

        // This code is pretty standard, except for the usage of
        // `GreenTask::convert`. Right now if we use `reawaken` directly it will
        // expect for there to be a task in local TLS, but that is not true for
        // this deschedule block (because the scheduler must retain ownership of
        // the task while the cleanup job is running). In order to get around
        // this for now, we invoke the scheduler directly with the converted
        // Task => GreenTask structure.
        if times == 1 {
            sched.deschedule_running_task_and_then(self, |sched, task| {
                match f(task) {
                    Ok(()) => {}
                    Err(t) => {
                        t.wake().map(|t| {
                            sched.enqueue_task(GreenTask::convert(t))
                        });
                    }
                }
            });
        } else {
            sched.deschedule_running_task_and_then(self, |sched, task| {
                for task in task.make_selectable(times) {
                    match f(task) {
                        Ok(()) => {},
                        Err(task) => {
                            task.wake().map(|t| {
                                sched.enqueue_task(GreenTask::convert(t))
                            });
                            break
                        }
                    }
                }
            });
        }
    }

    fn reawaken(mut self: Box<GreenTask>, to_wake: Box<Task>) {
        self.put_task(to_wake);
        assert!(self.sched.is_none());

        // Optimistically look for a local task, but if one's not available to
        // inspect (in order to see if it's in the same sched pool as we are),
        // then just use our remote wakeup routine and carry on!
        let mut running_task: Box<Task> = match Local::try_take() {
            Some(task) => task,
            None => return self.reawaken_remotely()
        };

        // Waking up a green thread is a bit of a tricky situation. We have no
        // guarantee about where the current task is running. The options we
        // have for where this current task is running are:
        //
        //  1. Our original scheduler pool
        //  2. Some other scheduler pool
        //  3. Something that isn't a scheduler pool
        //
        // In order to figure out what case we're in, this is the reason that
        // the `maybe_take_runtime` function exists. Using this function we can
        // dynamically check to see which of these cases is the current
        // situation and then dispatch accordingly.
        //
        // In case 1, we just use the local scheduler to resume ourselves
        // immediately (if a rescheduling is possible).
        //
        // In case 2 and 3, we need to remotely reawaken ourself in order to be
        // transplanted back to the correct scheduler pool.
        match running_task.maybe_take_runtime::<GreenTask>() {
            Some(mut running_green_task) => {
                running_green_task.put_task(running_task);
                let sched = running_green_task.sched.take().unwrap();

                if sched.pool_id == self.pool_id {
                    sched.run_task(running_green_task, self);
                } else {
                    self.reawaken_remotely();

                    // put that thing back where it came from!
                    running_green_task.put_with_sched(sched);
                }
            }
            None => {
                self.reawaken_remotely();
                Local::put(running_task);
            }
        }
    }

    fn spawn_sibling(mut self: Box<GreenTask>,
                     cur_task: Box<Task>,
                     opts: TaskOpts,
                     f: proc():Send) {
        self.put_task(cur_task);

        // First, set up a bomb which when it goes off will restore the local
        // task unless its disarmed. This will allow us to gracefully fail from
        // inside of `configure` which allocates a new task.
        struct Bomb { inner: Option<Box<GreenTask>> }
        impl Drop for Bomb {
            fn drop(&mut self) {
                let _ = self.inner.take().map(|task| task.put());
            }
        }
        let mut bomb = Bomb { inner: Some(self) };

        // Spawns a task into the current scheduler. We allocate the new task's
        // stack from the scheduler's stack pool, and then configure it
        // accordingly to `opts`. Afterwards we bootstrap it immediately by
        // switching to it.
        //
        // Upon returning, our task is back in TLS and we're good to return.
        let sibling = {
            let sched = bomb.inner.as_mut().unwrap().sched.as_mut().unwrap();
            GreenTask::configure(&mut sched.stack_pool, opts, f)
        };
        let mut me = bomb.inner.take().unwrap();
        let sched = me.sched.take().unwrap();
        sched.run_task(me, sibling)
    }

    // Local I/O is provided by the scheduler's event loop
    fn local_io<'a>(&'a mut self) -> Option<rtio::LocalIo<'a>> {
        match self.sched.as_mut().unwrap().event_loop.io() {
            Some(io) => Some(rtio::LocalIo::new(io)),
            None => None,
        }
    }

    fn stack_bounds(&self) -> (uint, uint) {
        let c = self.coroutine.as_ref()
            .expect("GreenTask.stack_bounds called without a coroutine");

        // Don't return the red zone as part of the usable stack of this task,
        // it's essentially an implementation detail.
        (c.current_stack_segment.start() as uint + stack::RED_ZONE,
         c.current_stack_segment.end() as uint)
    }

    fn stack_guard(&self) -> Option<uint> {
        let c = self.coroutine.as_ref()
            .expect("GreenTask.stack_guard called without a coroutine");

        Some(c.current_stack_segment.guard() as uint)
    }

    fn can_block(&self) -> bool { false }

    fn wrap(self: Box<GreenTask>) -> Box<Any+'static> {
        self as Box<Any+'static>
    }
}

#[cfg(test)]
mod tests {
    use std::rt::local::Local;
    use std::rt::task::Task;
    use std::task;
    use std::rt::task::TaskOpts;

    use super::super::{PoolConfig, SchedPool};
    use super::GreenTask;

    fn spawn_opts(opts: TaskOpts, f: proc():Send) {
        let mut pool = SchedPool::new(PoolConfig {
            threads: 1,
            event_loop_factory: super::super::basic::event_loop,
        });
        pool.spawn(opts, f);
        pool.shutdown();
    }

    #[test]
    fn smoke() {
        let (tx, rx) = channel();
        spawn_opts(TaskOpts::new(), proc() {
            tx.send(());
        });
        rx.recv();
    }

    #[test]
    fn smoke_fail() {
        let (tx, rx) = channel::<int>();
        spawn_opts(TaskOpts::new(), proc() {
            let _tx = tx;
            fail!()
        });
        assert_eq!(rx.recv_opt(), Err(()));
    }

    #[test]
    fn smoke_opts() {
        let mut opts = TaskOpts::new();
        opts.name = Some("test".into_maybe_owned());
        opts.stack_size = Some(20 * 4096);
        let (tx, rx) = channel();
        opts.on_exit = Some(proc(r) tx.send(r));
        spawn_opts(opts, proc() {});
        assert!(rx.recv().is_ok());
    }

    #[test]
    fn smoke_opts_fail() {
        let mut opts = TaskOpts::new();
        let (tx, rx) = channel();
        opts.on_exit = Some(proc(r) tx.send(r));
        spawn_opts(opts, proc() { fail!() });
        assert!(rx.recv().is_err());
    }

    #[test]
    fn yield_test() {
        let (tx, rx) = channel();
        spawn_opts(TaskOpts::new(), proc() {
            for _ in range(0u, 10) { task::deschedule(); }
            tx.send(());
        });
        rx.recv();
    }

    #[test]
    fn spawn_children() {
        let (tx1, rx) = channel();
        spawn_opts(TaskOpts::new(), proc() {
            let (tx2, rx) = channel();
            spawn(proc() {
                let (tx3, rx) = channel();
                spawn(proc() {
                    tx3.send(());
                });
                rx.recv();
                tx2.send(());
            });
            rx.recv();
            tx1.send(());
        });
        rx.recv();
    }

    #[test]
    fn spawn_inherits() {
        let (tx, rx) = channel();
        spawn_opts(TaskOpts::new(), proc() {
            spawn(proc() {
                let mut task: Box<Task> = Local::take();
                match task.maybe_take_runtime::<GreenTask>() {
                    Some(ops) => {
                        task.put_runtime(ops);
                    }
                    None => fail!(),
                }
                Local::put(task);
                tx.send(());
            });
        });
        rx.recv();
    }
}
