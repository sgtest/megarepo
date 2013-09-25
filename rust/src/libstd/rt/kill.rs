// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Task death: asynchronous killing, linked failure, exit code propagation.

This file implements two orthogonal building-blocks for communicating failure
between tasks. One is 'linked failure' or 'task killing', that is, a failing
task causing other tasks to fail promptly (even those that are blocked on
pipes or I/O). The other is 'exit code propagation', which affects the result
observed by the parent of a task::try task that itself spawns child tasks
(such as any #[test] function). In both cases the data structures live in
KillHandle.


I. Task killing.

The model for killing involves two atomic flags, the "kill flag" and the
"unkillable flag". Operations on the kill flag include:

- In the taskgroup code (task/spawn.rs), tasks store a clone of their
  KillHandle in their shared taskgroup. Another task in the group that fails
  will use that handle to call kill().
- When a task blocks, it turns its ~Task into a BlockedTask by storing a
  the transmuted ~Task pointer inside the KillHandle's kill flag. A task
  trying to block and a task trying to kill it can simultaneously access the
  kill flag, after which the task will get scheduled and fail (no matter who
  wins the race). Likewise, a task trying to wake a blocked task normally and
  a task trying to kill it can simultaneously access the flag; only one will
  get the task to reschedule it.

Operations on the unkillable flag include:

- When a task becomes unkillable, it swaps on the flag to forbid any killer
  from waking it up while it's blocked inside the unkillable section. If a
  kill was already pending, the task fails instead of becoming unkillable.
- When a task is done being unkillable, it restores the flag to the normal
  running state. If a kill was received-but-blocked during the unkillable
  section, the task fails at this later point.
- When a task tries to kill another task, before swapping on the kill flag, it
  first swaps on the unkillable flag, to see if it's "allowed" to wake up the
  task. If it isn't, the killed task will receive the signal when it becomes
  killable again. (Of course, a task trying to wake the task normally (e.g.
  sending on a channel) does not access the unkillable flag at all.)

Why do we not need acquire/release barriers on any of the kill flag swaps?
This is because barriers establish orderings between accesses on different
memory locations, but each kill-related operation is only a swap on a single
location, so atomicity is all that matters. The exception is kill(), which
does a swap on both flags in sequence. kill() needs no barriers because it
does not matter if its two accesses are seen reordered on another CPU: if a
killer does perform both writes, it means it saw a KILL_RUNNING in the
unkillable flag, which means an unkillable task will see KILL_KILLED and fail
immediately (rendering the subsequent write to the kill flag unnecessary).


II. Exit code propagation.

The basic model for exit code propagation, which is used with the "watched"
spawn mode (on by default for linked spawns, off for supervised and unlinked
spawns), is that a parent will wait for all its watched children to exit
before reporting whether it succeeded or failed. A watching parent will only
report success if it succeeded and all its children also reported success;
otherwise, it will report failure. This is most useful for writing test cases:

 ```
#[test]
fn test_something_in_another_task {
    do spawn {
        assert!(collatz_conjecture_is_false());
    }
}
 ```

Here, as the child task will certainly outlive the parent task, we might miss
the failure of the child when deciding whether or not the test case passed.
The watched spawn mode avoids this problem.

In order to propagate exit codes from children to their parents, any
'watching' parent must wait for all of its children to exit before it can
report its final exit status. We achieve this by using an UnsafeArc, using the
reference counting to track how many children are still alive, and using the
unwrap() operation in the parent's exit path to wait for all children to exit.
The UnsafeArc referred to here is actually the KillHandle itself.

This also works transitively, as if a "middle" watched child task is itself
watching a grandchild task, the "middle" task will do unwrap() on its own
KillHandle (thereby waiting for the grandchild to exit) before dropping its
reference to its watching parent (which will alert the parent).

While UnsafeArc::unwrap() accomplishes the synchronization, there remains the
matter of reporting the exit codes themselves. This is easiest when an exiting
watched task has no watched children of its own:

- If the task with no watched children exits successfully, it need do nothing.
- If the task with no watched children has failed, it sets a flag in the
  parent's KillHandle ("any_child_failed") to false. It then stays false forever.

However, if a "middle" watched task with watched children of its own exits
before its child exits, we need to ensure that the grandparent task may still
see a failure from the grandchild task. While we could achieve this by having
each intermediate task block on its handle, this keeps around the other resources
the task was using. To be more efficient, this is accomplished via "tombstones".

A tombstone is a closure, ~fn() -> bool, which will perform any waiting necessary
to collect the exit code of descendant tasks. In its environment is captured
the KillHandle of whichever task created the tombstone, and perhaps also any
tombstones that that task itself had, and finally also another tombstone,
effectively creating a lazy-list of heap closures.

When a child wishes to exit early and leave tombstones behind for its parent,
it must use a LittleLock (pthread mutex) to synchronize with any possible
sibling tasks which are trying to do the same thing with the same parent.
However, on the other side, when the parent is ready to pull on the tombstones,
it need not use this lock, because the unwrap() serves as a barrier that ensures
no children will remain with references to the handle.

The main logic for creating and assigning tombstones can be found in the
function reparent_children_to() in the impl for KillHandle.


IIA. Issues with exit code propagation.

There are two known issues with the current scheme for exit code propagation.

- As documented in issue #8136, the structure mandates the possibility for stack
  overflow when collecting tombstones that are very deeply nested. This cannot
  be avoided with the closure representation, as tombstones end up structured in
  a sort of tree. However, notably, the tombstones do not actually need to be
  collected in any particular order, and so a doubly-linked list may be used.
  However we do not do this yet because DList is in libextra.

- A discussion with Graydon made me realize that if we decoupled the exit code
  propagation from the parents-waiting action, this could result in a simpler
  implementation as the exit codes themselves would not have to be propagated,
  and could instead be propagated implicitly through the taskgroup mechanism
  that we already have. The tombstoning scheme would still be required. I have
  not implemented this because currently we can't receive a linked failure kill
  signal during the task cleanup activity, as that is currently "unkillable",
  and occurs outside the task's unwinder's "try" block, so would require some
  restructuring.

*/

use cast;
use cell::Cell;
use either::{Either, Left, Right};
use option::{Option, Some, None};
use prelude::*;
use rt::task::Task;
use task::spawn::Taskgroup;
use to_bytes::IterBytes;
use unstable::atomics::{AtomicUint, Relaxed};
use unstable::sync::{UnsafeArc, LittleLock};
use util;

static KILLED_MSG: &'static str = "killed by linked failure";

// State values for the 'killed' and 'unkillable' atomic flags below.
static KILL_RUNNING:    uint = 0;
static KILL_KILLED:     uint = 1;
static KILL_UNKILLABLE: uint = 2;

struct KillFlag(AtomicUint);
type KillFlagHandle = UnsafeArc<KillFlag>;

/// A handle to a blocked task. Usually this means having the ~Task pointer by
/// ownership, but if the task is killable, a killer can steal it at any time.
pub enum BlockedTask {
    Unkillable(~Task),
    Killable(KillFlagHandle),
}

// FIXME(#7544)(bblum): think about the cache efficiency of this
struct KillHandleInner {
    // Is the task running, blocked, or killed? Possible values:
    // * KILL_RUNNING    - Not unkillable, no kill pending.
    // * KILL_KILLED     - Kill pending.
    // * <ptr>           - A transmuted blocked ~Task pointer.
    // This flag is refcounted because it may also be referenced by a blocking
    // concurrency primitive, used to wake the task normally, whose reference
    // may outlive the handle's if the task is killed.
    killed: KillFlagHandle,
    // Has the task deferred kill signals? This flag guards the above one.
    // Possible values:
    // * KILL_RUNNING    - Not unkillable, no kill pending.
    // * KILL_KILLED     - Kill pending.
    // * KILL_UNKILLABLE - Kill signals deferred.
    unkillable: AtomicUint,

    // Shared state between task and children for exit code propagation. These
    // are here so we can re-use the kill handle to implement watched children
    // tasks. Using a separate Arc-like would introduce extra atomic adds/subs
    // into common spawn paths, so this is just for speed.

    // Locklessly accessed; protected by the enclosing refcount's barriers.
    any_child_failed: bool,
    // A lazy list, consuming which may unwrap() many child tombstones.
    child_tombstones: Option<~fn() -> bool>,
    // Protects multiple children simultaneously creating tombstones.
    graveyard_lock: LittleLock,
}

/// State shared between tasks used for task killing during linked failure.
#[deriving(Clone)]
pub struct KillHandle(UnsafeArc<KillHandleInner>);

/// Per-task state related to task death, killing, failure, etc.
pub struct Death {
    // Shared among this task, its watched children, and any linked tasks who
    // might kill it. This is optional so we can take it by-value at exit time.
    kill_handle:     Option<KillHandle>,
    // Handle to a watching parent, if we have one, for exit code propagation.
    watching_parent: Option<KillHandle>,
    // Action to be done with the exit code. If set, also makes the task wait
    // until all its watched children exit before collecting the status.
    on_exit:         Option<~fn(bool)>,
    // nesting level counter for task::unkillable calls (0 == killable).
    unkillable:      int,
    // nesting level counter for unstable::atomically calls (0 == can deschedule).
    wont_sleep:      int,
    // A "spare" handle to the kill flag inside the kill handle. Used during
    // blocking/waking as an optimization to avoid two xadds on the refcount.
    spare_kill_flag: Option<KillFlagHandle>,
}

impl Drop for KillFlag {
    // Letting a KillFlag with a task inside get dropped would leak the task.
    // We could free it here, but the task should get awoken by hand somehow.
    fn drop(&mut self) {
        match self.load(Relaxed) {
            KILL_RUNNING | KILL_KILLED => { },
            _ => rtabort!("can't drop kill flag with a blocked task inside!"),
        }
    }
}

// Whenever a task blocks, it swaps out its spare kill flag to use as the
// blocked task handle. So unblocking a task must restore that spare.
unsafe fn revive_task_ptr(task_ptr: uint, spare_flag: Option<KillFlagHandle>) -> ~Task {
    let mut task: ~Task = cast::transmute(task_ptr);
    if task.death.spare_kill_flag.is_none() {
        task.death.spare_kill_flag = spare_flag;
    } else {
        // A task's spare kill flag is not used for blocking in one case:
        // when an unkillable task blocks on select. In this case, a separate
        // one was created, which we now discard.
        rtassert!(task.death.unkillable > 0);
    }
    task
}

impl BlockedTask {
    /// Returns Some if the task was successfully woken; None if already killed.
    pub fn wake(self) -> Option<~Task> {
        match self {
            Unkillable(task) => Some(task),
            Killable(flag_arc) => {
                let flag = unsafe { &mut **flag_arc.get() };
                match flag.swap(KILL_RUNNING, Relaxed) {
                    KILL_RUNNING => None, // woken from select(), perhaps
                    KILL_KILLED  => None, // a killer stole it already
                    task_ptr     =>
                        Some(unsafe { revive_task_ptr(task_ptr, Some(flag_arc)) })
                }
            }
        }
    }

    /// Create a blocked task, unless the task was already killed.
    pub fn try_block(mut task: ~Task) -> Either<~Task, BlockedTask> {
        // NB: As an optimization, we could give a free pass to being unkillable
        // to tasks whose taskgroups haven't been initialized yet, but that
        // introduces complications with select() and with the test cases below,
        // and it's not clear the uncommon performance boost is worth it.
        if task.death.unkillable > 0 {
            Right(Unkillable(task))
        } else {
            rtassert!(task.death.kill_handle.is_some());
            unsafe {
                // The inverse of 'revive', above, occurs here.
                // The spare kill flag will usually be Some, unless the task was
                // already killed, in which case the killer will have deferred
                // creating a new one until whenever it blocks during unwinding.
                let flag_arc = match task.death.spare_kill_flag.take() {
                    Some(spare_flag) => spare_flag,
                    None => {
                        // A task that kills us won't have a spare kill flag to
                        // give back to us, so we restore it ourselves here. This
                        // situation should only arise when we're already failing.
                        rtassert!(task.unwinder.unwinding);
                        (*task.death.kill_handle.get_ref().get()).killed.clone()
                    }
                };
                let flag     = &mut **flag_arc.get();
                let task_ptr = cast::transmute(task);
                // Expect flag to contain RUNNING. If KILLED, it should stay KILLED.
                match flag.compare_and_swap(KILL_RUNNING, task_ptr, Relaxed) {
                    KILL_RUNNING => Right(Killable(flag_arc)),
                    KILL_KILLED  => Left(revive_task_ptr(task_ptr, Some(flag_arc))),
                    x            => rtabort!("can't block task! kill flag = %?", x),
                }
            }
        }
    }

    /// Converts one blocked task handle to a list of many handles to the same.
    pub fn make_selectable(self, num_handles: uint) -> ~[BlockedTask] {
        let handles = match self {
            Unkillable(task) => {
                let flag = unsafe { KillFlag(AtomicUint::new(cast::transmute(task))) };
                UnsafeArc::newN(flag, num_handles)
            }
            Killable(flag_arc) => flag_arc.cloneN(num_handles),
        };
        // Even if the task was unkillable before, we use 'Killable' because
        // multiple pipes will have handles. It does not really mean killable.
        handles.move_iter().map(|x| Killable(x)).collect()
    }

    // This assertion has two flavours because the wake involves an atomic op.
    // In the faster version, destructors will fail dramatically instead.
    #[inline] #[cfg(not(test))]
    pub fn assert_already_awake(self) { }
    #[inline] #[cfg(test)]
    pub fn assert_already_awake(self) { assert!(self.wake().is_none()); }

    /// Convert to an unsafe uint value. Useful for storing in a pipe's state flag.
    #[inline]
    pub unsafe fn cast_to_uint(self) -> uint {
        // Use the low bit to distinguish the enum variants, to save a second
        // allocation in the indestructible case.
        match self {
            Unkillable(task) => {
                let blocked_task_ptr: uint = cast::transmute(task);
                rtassert!(blocked_task_ptr & 0x1 == 0);
                blocked_task_ptr
            },
            Killable(flag_arc) => {
                let blocked_task_ptr: uint = cast::transmute(~flag_arc);
                rtassert!(blocked_task_ptr & 0x1 == 0);
                blocked_task_ptr | 0x1
            }
        }
    }

    /// Convert from an unsafe uint value. Useful for retrieving a pipe's state flag.
    #[inline]
    pub unsafe fn cast_from_uint(blocked_task_ptr: uint) -> BlockedTask {
        if blocked_task_ptr & 0x1 == 0 {
            Unkillable(cast::transmute(blocked_task_ptr))
        } else {
            let ptr: ~KillFlagHandle = cast::transmute(blocked_task_ptr & !0x1);
            match ptr {
                ~flag_arc => Killable(flag_arc)
            }
        }
    }
}

// So that KillHandle can be hashed in the taskgroup bookkeeping code.
impl IterBytes for KillHandle {
    fn iter_bytes(&self, lsb0: bool, f: &fn(buf: &[u8]) -> bool) -> bool {
        self.data.iter_bytes(lsb0, f)
    }
}
impl Eq for KillHandle {
    #[inline] fn eq(&self, other: &KillHandle) -> bool { self.data.eq(&other.data) }
    #[inline] fn ne(&self, other: &KillHandle) -> bool { self.data.ne(&other.data) }
}

impl KillHandle {
    pub fn new() -> (KillHandle, KillFlagHandle) {
        let (flag, flag_clone) =
            UnsafeArc::new2(KillFlag(AtomicUint::new(KILL_RUNNING)));
        let handle = KillHandle(UnsafeArc::new(KillHandleInner {
            // Linked failure fields
            killed:     flag,
            unkillable: AtomicUint::new(KILL_RUNNING),
            // Exit code propagation fields
            any_child_failed: false,
            child_tombstones: None,
            graveyard_lock:   LittleLock::new(),
        }));
        (handle, flag_clone)
    }

    // Will begin unwinding if a kill signal was received, unless already_failing.
    // This can't be used recursively, because a task which sees a KILLED
    // signal must fail immediately, which an already-unkillable task can't do.
    #[inline]
    pub fn inhibit_kill(&mut self, already_failing: bool) {
        let inner = unsafe { &mut *self.get() };
        // Expect flag to contain RUNNING. If KILLED, it should stay KILLED.
        // FIXME(#7544)(bblum): is it really necessary to prohibit double kill?
        match inner.unkillable.compare_and_swap(KILL_RUNNING, KILL_UNKILLABLE, Relaxed) {
            KILL_RUNNING    => { }, // normal case
            KILL_KILLED     => if !already_failing { fail!(KILLED_MSG) },
            _               => rtabort!("inhibit_kill: task already unkillable"),
        }
    }

    // Will begin unwinding if a kill signal was received, unless already_failing.
    #[inline]
    pub fn allow_kill(&mut self, already_failing: bool) {
        let inner = unsafe { &mut *self.get() };
        // Expect flag to contain UNKILLABLE. If KILLED, it should stay KILLED.
        // FIXME(#7544)(bblum): is it really necessary to prohibit double kill?
        match inner.unkillable.compare_and_swap(KILL_UNKILLABLE, KILL_RUNNING, Relaxed) {
            KILL_UNKILLABLE => { }, // normal case
            KILL_KILLED     => if !already_failing { fail!(KILLED_MSG) },
            _               => rtabort!("allow_kill: task already killable"),
        }
    }

    // Send a kill signal to the handle's owning task. Returns the task itself
    // if it was blocked and needs punted awake. To be called by other tasks.
    pub fn kill(&mut self) -> Option<~Task> {
        let inner = unsafe { &mut *self.get() };
        if inner.unkillable.swap(KILL_KILLED, Relaxed) == KILL_RUNNING {
            // Got in. Allowed to try to punt the task awake.
            let flag = unsafe { &mut *inner.killed.get() };
            match flag.swap(KILL_KILLED, Relaxed) {
                // Task either not blocked or already taken care of.
                KILL_RUNNING | KILL_KILLED => None,
                // Got ownership of the blocked task.
                // While the usual 'wake' path can just pass back the flag
                // handle, we (the slower kill path) haven't an extra one lying
                // around. The task will wake up without a spare.
                task_ptr => Some(unsafe { revive_task_ptr(task_ptr, None) }),
            }
        } else {
            // Otherwise it was either unkillable or already killed. Somebody
            // else was here first who will deal with the kill signal.
            None
        }
    }

    #[inline]
    pub fn killed(&self) -> bool {
        // Called every context switch, so shouldn't report true if the task
        // is unkillable with a kill signal pending.
        let inner = unsafe { &*self.get() };
        let flag  = unsafe { &*inner.killed.get() };
        // A barrier-related concern here is that a task that gets killed
        // awake needs to see the killer's write of KILLED to this flag. This
        // is analogous to receiving a pipe payload; the appropriate barrier
        // should happen when enqueueing the task.
        flag.load(Relaxed) == KILL_KILLED
    }

    pub fn notify_immediate_failure(&mut self) {
        // A benign data race may happen here if there are failing sibling
        // tasks that were also spawned-watched. The refcount's write barriers
        // in UnsafeArc ensure that this write will be seen by the
        // unwrapper/destructor, whichever task may unwrap it.
        unsafe { (*self.get()).any_child_failed = true; }
    }

    // For use when a task does not need to collect its children's exit
    // statuses, but the task has a parent which might want them.
    pub fn reparent_children_to(self, parent: &mut KillHandle) {
        // Optimistic path: If another child of the parent's already failed,
        // we don't need to worry about any of this.
        if unsafe { (*parent.get()).any_child_failed } {
            return;
        }

        // Try to see if all our children are gone already.
        match self.try_unwrap() {
            // Couldn't unwrap; children still alive. Reparent entire handle as
            // our own tombstone, to be unwrapped later.
            Left(this) => {
                let this = Cell::new(this); // :(
                do add_lazy_tombstone(parent) |other_tombstones| {
                    let this = Cell::new(this.take()); // :(
                    let others = Cell::new(other_tombstones); // :(
                    || {
                        // Prefer to check tombstones that were there first,
                        // being "more fair" at the expense of tail-recursion.
                        others.take().map_move_default(true, |f| f()) && {
                            let mut inner = this.take().unwrap();
                            (!inner.any_child_failed) &&
                                inner.child_tombstones.take().map_move_default(true, |f| f())
                        }
                    }
                }
            }
            // Whether or not all children exited, one or more already failed.
            Right(KillHandleInner { any_child_failed: true, _ }) => {
                parent.notify_immediate_failure();
            }
            // All children exited, but some left behind tombstones that we
            // don't want to wait on now. Give them to our parent.
            Right(KillHandleInner { any_child_failed: false,
                                    child_tombstones: Some(f), _ }) => {
                let f = Cell::new(f); // :(
                do add_lazy_tombstone(parent) |other_tombstones| {
                    let f = Cell::new(f.take()); // :(
                    let others = Cell::new(other_tombstones); // :(
                    || {
                        // Prefer fairness to tail-recursion, as in above case.
                        others.take().map_move_default(true, |f| f()) &&
                            f.take()()
                    }
                }
            }
            // All children exited, none failed. Nothing to do!
            Right(KillHandleInner { any_child_failed: false,
                                    child_tombstones: None, _ }) => { }
        }

        // NB: Takes a pthread mutex -- 'blk' not allowed to reschedule.
        #[inline]
        fn add_lazy_tombstone(parent: &mut KillHandle,
                              blk: &fn(Option<~fn() -> bool>) -> ~fn() -> bool) {

            let inner: &mut KillHandleInner = unsafe { &mut *parent.get() };
            unsafe {
                do inner.graveyard_lock.lock {
                    // Update the current "head node" of the lazy list.
                    inner.child_tombstones =
                        Some(blk(util::replace(&mut inner.child_tombstones, None)));
                }
            }
        }
    }
}

impl Death {
    pub fn new() -> Death {
        let (handle, spare) = KillHandle::new();
        Death {
            kill_handle:     Some(handle),
            watching_parent: None,
            on_exit:         None,
            unkillable:      0,
            wont_sleep:      0,
            spare_kill_flag: Some(spare),
        }
    }

    pub fn new_child(&self) -> Death {
        // FIXME(#7327)
        let (handle, spare) = KillHandle::new();
        Death {
            kill_handle:     Some(handle),
            watching_parent: self.kill_handle.clone(),
            on_exit:         None,
            unkillable:      0,
            wont_sleep:      0,
            spare_kill_flag: Some(spare),
        }
    }

    /// Collect failure exit codes from children and propagate them to a parent.
    pub fn collect_failure(&mut self, mut success: bool, group: Option<Taskgroup>) {
        // This may run after the task has already failed, so even though the
        // task appears to need to be killed, the scheduler should not fail us
        // when we block to unwrap.
        // (XXX: Another less-elegant reason for doing this is so that the use
        // of the LittleLock in reparent_children_to doesn't need to access the
        // unkillable flag in the kill_handle, since we'll have removed it.)
        rtassert!(self.unkillable == 0);
        self.unkillable = 1;

        // NB. See corresponding comment at the callsite in task.rs.
        // FIXME(#8192): Doesn't work with "let _ = ..."
        { use util; util::ignore(group); }

        // Step 1. Decide if we need to collect child failures synchronously.
        do self.on_exit.take().map_move |on_exit| {
            if success {
                // We succeeded, but our children might not. Need to wait for them.
                let mut inner = self.kill_handle.take_unwrap().unwrap();
                if inner.any_child_failed {
                    success = false;
                } else {
                    // Lockless access to tombstones protected by unwrap barrier.
                    success = inner.child_tombstones.take().map_move_default(true, |f| f());
                }
            }
            on_exit(success);
        };

        // Step 2. Possibly alert possibly-watching parent to failure status.
        // Note that as soon as parent_handle goes out of scope, the parent
        // can successfully unwrap its handle and collect our reported status.
        do self.watching_parent.take().map_move |mut parent_handle| {
            if success {
                // Our handle might be None if we had an exit callback, and
                // already unwrapped it. But 'success' being true means no
                // child failed, so there's nothing to do (see below case).
                do self.kill_handle.take().map_move |own_handle| {
                    own_handle.reparent_children_to(&mut parent_handle);
                };
            } else {
                // Can inform watching parent immediately that we failed.
                // (Note the importance of non-failing tasks NOT writing
                // 'false', which could obscure another task's failure.)
                parent_handle.notify_immediate_failure();
            }
        };

        // Can't use allow_kill directly; that would require the kill handle.
        rtassert!(self.unkillable == 1);
        self.unkillable = 0;
    }

    /// Fails if a kill signal was received.
    #[inline]
    pub fn check_killed(&self, already_failing: bool) {
        match self.kill_handle {
            Some(ref kill_handle) =>
                // The task may be both unkillable and killed if it does some
                // synchronization during unwinding or cleanup (for example,
                // sending on a notify port). In that case failing won't help.
                if self.unkillable == 0 && (!already_failing) && kill_handle.killed() {
                    fail!(KILLED_MSG);
                },
            // This may happen during task death (see comments in collect_failure).
            None => rtassert!(self.unkillable > 0),
        }
    }

    /// Enter a possibly-nested unkillable section of code.
    /// All calls must be paired with a subsequent call to allow_kill.
    #[inline]
    pub fn inhibit_kill(&mut self, already_failing: bool) {
        self.unkillable += 1;
        // May fail, hence must happen *after* incrementing the counter
        if self.unkillable == 1 {
            rtassert!(self.kill_handle.is_some());
            self.kill_handle.get_mut_ref().inhibit_kill(already_failing);
        }
    }

    /// Exit a possibly-nested unkillable section of code.
    /// All calls must be paired with a preceding call to inhibit_kill.
    #[inline]
    pub fn allow_kill(&mut self, already_failing: bool) {
        if self.unkillable == 0 {
            // we need to decrement the counter before failing.
            self.unkillable -= 1;
            fail!("Cannot enter a rekillable() block without a surrounding unkillable()");
        }
        self.unkillable -= 1;
        if self.unkillable == 0 {
            rtassert!(self.kill_handle.is_some());
            self.kill_handle.get_mut_ref().allow_kill(already_failing);
        }
    }

    /// Enter a possibly-nested "atomic" section of code. Just for assertions.
    /// All calls must be paired with a subsequent call to allow_deschedule.
    #[inline]
    pub fn inhibit_deschedule(&mut self) {
        self.wont_sleep += 1;
    }

    /// Exit a possibly-nested "atomic" section of code. Just for assertions.
    /// All calls must be paired with a preceding call to inhibit_deschedule.
    #[inline]
    pub fn allow_deschedule(&mut self) {
        rtassert!(self.wont_sleep != 0);
        self.wont_sleep -= 1;
    }

    /// Ensure that the task is allowed to become descheduled.
    #[inline]
    pub fn assert_may_sleep(&self) {
        if self.wont_sleep != 0 {
            rtabort!("illegal atomic-sleep: attempt to reschedule while \
                      using an Exclusive or LittleLock");
        }
    }
}

impl Drop for Death {
    fn drop(&mut self) {
        // Mustn't be in an atomic or unkillable section at task death.
        rtassert!(self.unkillable == 0);
        rtassert!(self.wont_sleep == 0);
    }
}

#[cfg(test)]
mod test {
    #[allow(unused_mut)];
    use cell::Cell;
    use rt::test::*;
    use super::*;
    use util;

    // Test cases don't care about the spare killed flag.
    fn make_kill_handle() -> KillHandle { let (h,_) = KillHandle::new(); h }

    #[ignore(reason = "linked failure")]
    #[test]
    fn no_tombstone_success() {
        do run_in_newsched_task {
            // Tests case 4 of the 4-way match in reparent_children.
            let mut parent = make_kill_handle();
            let mut child  = make_kill_handle();

            // Without another handle to child, the try unwrap should succeed.
            child.reparent_children_to(&mut parent);
            let mut parent_inner = parent.unwrap();
            assert!(parent_inner.child_tombstones.is_none());
            assert!(parent_inner.any_child_failed == false);
        }
    }
    #[test]
    fn no_tombstone_failure() {
        do run_in_newsched_task {
            // Tests case 2 of the 4-way match in reparent_children.
            let mut parent = make_kill_handle();
            let mut child  = make_kill_handle();

            child.notify_immediate_failure();
            // Without another handle to child, the try unwrap should succeed.
            child.reparent_children_to(&mut parent);
            let mut parent_inner = parent.unwrap();
            assert!(parent_inner.child_tombstones.is_none());
            // Immediate failure should have been propagated.
            assert!(parent_inner.any_child_failed);
        }
    }
    #[test]
    fn no_tombstone_because_sibling_already_failed() {
        do run_in_newsched_task {
            // Tests "case 0, the optimistic path in reparent_children.
            let mut parent = make_kill_handle();
            let mut child1 = make_kill_handle();
            let mut child2 = make_kill_handle();
            let mut link   = child2.clone();

            // Should set parent's child_failed flag
            child1.notify_immediate_failure();
            child1.reparent_children_to(&mut parent);
            // Should bypass trying to unwrap child2 entirely.
            // Otherwise, due to 'link', it would try to tombstone.
            child2.reparent_children_to(&mut parent);
            // Should successfully unwrap even though 'link' is still alive.
            let mut parent_inner = parent.unwrap();
            assert!(parent_inner.child_tombstones.is_none());
            // Immediate failure should have been propagated by first child.
            assert!(parent_inner.any_child_failed);
            util::ignore(link);
        }
    }
    #[test]
    fn one_tombstone_success() {
        do run_in_newsched_task {
            let mut parent = make_kill_handle();
            let mut child  = make_kill_handle();
            let mut link   = child.clone();

            // Creates 1 tombstone. Existence of 'link' makes try-unwrap fail.
            child.reparent_children_to(&mut parent);
            // Let parent collect tombstones.
            util::ignore(link);
            // Must have created a tombstone
            let mut parent_inner = parent.unwrap();
            assert!(parent_inner.child_tombstones.take_unwrap()());
            assert!(parent_inner.any_child_failed == false);
        }
    }
    #[test]
    fn one_tombstone_failure() {
        do run_in_newsched_task {
            let mut parent = make_kill_handle();
            let mut child  = make_kill_handle();
            let mut link   = child.clone();

            // Creates 1 tombstone. Existence of 'link' makes try-unwrap fail.
            child.reparent_children_to(&mut parent);
            // Must happen after tombstone to not be immediately propagated.
            link.notify_immediate_failure();
            // Let parent collect tombstones.
            util::ignore(link);
            // Must have created a tombstone
            let mut parent_inner = parent.unwrap();
            // Failure must be seen in the tombstone.
            assert!(parent_inner.child_tombstones.take_unwrap()() == false);
            assert!(parent_inner.any_child_failed == false);
        }
    }
    #[test]
    fn two_tombstones_success() {
        do run_in_newsched_task {
            let mut parent = make_kill_handle();
            let mut middle = make_kill_handle();
            let mut child  = make_kill_handle();
            let mut link   = child.clone();

            child.reparent_children_to(&mut middle); // case 1 tombstone
            // 'middle' should try-unwrap okay, but still have to reparent.
            middle.reparent_children_to(&mut parent); // case 3 tombston
            // Let parent collect tombstones.
            util::ignore(link);
            // Must have created a tombstone
            let mut parent_inner = parent.unwrap();
            assert!(parent_inner.child_tombstones.take_unwrap()());
            assert!(parent_inner.any_child_failed == false);
        }
    }
    #[test]
    fn two_tombstones_failure() {
        do run_in_newsched_task {
            let mut parent = make_kill_handle();
            let mut middle = make_kill_handle();
            let mut child  = make_kill_handle();
            let mut link   = child.clone();

            child.reparent_children_to(&mut middle); // case 1 tombstone
            // Must happen after tombstone to not be immediately propagated.
            link.notify_immediate_failure();
            // 'middle' should try-unwrap okay, but still have to reparent.
            middle.reparent_children_to(&mut parent); // case 3 tombstone
            // Let parent collect tombstones.
            util::ignore(link);
            // Must have created a tombstone
            let mut parent_inner = parent.unwrap();
            // Failure must be seen in the tombstone.
            assert!(parent_inner.child_tombstones.take_unwrap()() == false);
            assert!(parent_inner.any_child_failed == false);
        }
    }

    // Task killing tests

    #[test]
    fn kill_basic() {
        do run_in_newsched_task {
            let mut handle = make_kill_handle();
            assert!(!handle.killed());
            assert!(handle.kill().is_none());
            assert!(handle.killed());
        }
    }

    #[test]
    fn double_kill() {
        do run_in_newsched_task {
            let mut handle = make_kill_handle();
            assert!(!handle.killed());
            assert!(handle.kill().is_none());
            assert!(handle.killed());
            assert!(handle.kill().is_none());
            assert!(handle.killed());
        }
    }

    #[test]
    fn unkillable_after_kill() {
        do run_in_newsched_task {
            let mut handle = make_kill_handle();
            assert!(handle.kill().is_none());
            assert!(handle.killed());
            let handle_cell = Cell::new(handle);
            let result = do spawntask_try {
                handle_cell.take().inhibit_kill(false);
            };
            assert!(result.is_err());
        }
    }

    #[test]
    fn unkillable_during_kill() {
        do run_in_newsched_task {
            let mut handle = make_kill_handle();
            handle.inhibit_kill(false);
            assert!(handle.kill().is_none());
            assert!(!handle.killed());
            let handle_cell = Cell::new(handle);
            let result = do spawntask_try {
                handle_cell.take().allow_kill(false);
            };
            assert!(result.is_err());
        }
    }

    #[test]
    fn unkillable_before_kill() {
        do run_in_newsched_task {
            let mut handle = make_kill_handle();
            handle.inhibit_kill(false);
            handle.allow_kill(false);
            assert!(handle.kill().is_none());
            assert!(handle.killed());
        }
    }

    // Task blocking tests

    #[test]
    fn block_and_wake() {
        do with_test_task |mut task| {
            BlockedTask::try_block(task).unwrap_right().wake().unwrap()
        }
    }

    #[ignore(reason = "linked failure")]
    #[test]
    fn block_and_get_killed() {
        do with_test_task |mut task| {
            let mut handle = task.death.kill_handle.get_ref().clone();
            let result = BlockedTask::try_block(task).unwrap_right();
            let task = handle.kill().unwrap();
            assert!(result.wake().is_none());
            task
        }
    }

    #[ignore(reason = "linked failure")]
    #[test]
    fn block_already_killed() {
        do with_test_task |mut task| {
            let mut handle = task.death.kill_handle.get_ref().clone();
            assert!(handle.kill().is_none());
            BlockedTask::try_block(task).unwrap_left()
        }
    }

    #[ignore(reason = "linked failure")]
    #[test]
    fn block_unkillably_and_get_killed() {
        do with_test_task |mut task| {
            let mut handle = task.death.kill_handle.get_ref().clone();
            task.death.inhibit_kill(false);
            let result = BlockedTask::try_block(task).unwrap_right();
            assert!(handle.kill().is_none());
            let mut task = result.wake().unwrap();
            // This call wants to fail, but we can't have that happen since
            // we're not running in a newsched task, so we can't even use
            // spawntask_try. But the failing behaviour is already tested
            // above, in unkillable_during_kill(), so we punt on it here.
            task.death.allow_kill(true);
            task
        }
    }

    #[ignore(reason = "linked failure")]
    #[test]
    fn block_on_pipe() {
        // Tests the "killable" path of casting to/from uint.
        do run_in_newsched_task {
            do with_test_task |mut task| {
                let result = BlockedTask::try_block(task).unwrap_right();
                let result = unsafe { result.cast_to_uint() };
                let result = unsafe { BlockedTask::cast_from_uint(result) };
                result.wake().unwrap()
            }
        }
    }

    #[ignore(reason = "linked failure")]
    #[test]
    fn block_unkillably_on_pipe() {
        // Tests the "indestructible" path of casting to/from uint.
        do run_in_newsched_task {
            do with_test_task |mut task| {
                task.death.inhibit_kill(false);
                let result = BlockedTask::try_block(task).unwrap_right();
                let result = unsafe { result.cast_to_uint() };
                let result = unsafe { BlockedTask::cast_from_uint(result) };
                let mut task = result.wake().unwrap();
                task.death.allow_kill(false);
                task
            }
        }
    }
}
