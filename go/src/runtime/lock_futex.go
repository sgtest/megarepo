// Copyright 2011 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//go:build dragonfly || freebsd || linux

package runtime

import (
	"internal/runtime/atomic"
	"unsafe"
)

// This implementation depends on OS-specific implementations of
//
//	futexsleep(addr *uint32, val uint32, ns int64)
//		Atomically,
//			if *addr == val { sleep }
//		Might be woken up spuriously; that's allowed.
//		Don't sleep longer than ns; ns < 0 means forever.
//
//	futexwakeup(addr *uint32, cnt uint32)
//		If any procs are sleeping on addr, wake up at most cnt.

const (
	mutex_locked   = 0x1
	mutex_sleeping = 0x2 // Ensure futex's low 32 bits won't be all zeros

	active_spin     = 4
	active_spin_cnt = 30
	passive_spin    = 1
)

// The mutex.key holds two state flags in its lowest bits: When the mutex_locked
// bit is set, the mutex is locked. When the mutex_sleeping bit is set, a thread
// is waiting in futexsleep for the mutex to be available. These flags operate
// independently: a thread can enter lock2, observe that another thread is
// already asleep, and immediately try to grab the lock anyway without waiting
// for its "fair" turn.
//
// The rest of mutex.key holds a pointer to the head of a linked list of the Ms
// that are waiting for the mutex. The pointer portion is set if and only if the
// mutex_sleeping flag is set. Because the futex syscall operates on 32 bits but
// a uintptr may be larger, the flag lets us be sure the futexsleep call will
// only commit if the pointer portion is unset. Otherwise an M allocated at an
// address like 0x123_0000_0000 might miss its wakeups.

// We use the uintptr mutex.key and note.key as a uint32.
//
//go:nosplit
func key32(p *uintptr) *uint32 {
	return (*uint32)(unsafe.Pointer(p))
}

func mutexContended(l *mutex) bool {
	return atomic.Load(key32(&l.key)) > mutex_locked
}

func lock(l *mutex) {
	lockWithRank(l, getLockRank(l))
}

func lock2(l *mutex) {
	gp := getg()
	if gp.m.locks < 0 {
		throw("runtime·lock: lock count")
	}
	gp.m.locks++

	// Speculative grab for lock.
	if atomic.Casuintptr(&l.key, 0, mutex_locked) {
		return
	}

	// If a goroutine's stack needed to grow during a lock2 call, the M could
	// end up with two active lock2 calls (one each on curg and g0). If both are
	// contended, the call on g0 will corrupt mWaitList. Disable stack growth.
	stackguard0, throwsplit := gp.stackguard0, gp.throwsplit
	if gp == gp.m.curg {
		gp.stackguard0, gp.throwsplit = stackPreempt, true
	}

	var startNanos int64
	const sampleRate = gTrackingPeriod
	sample := cheaprandn(sampleRate) == 0
	if sample {
		startNanos = nanotime()
	}
	gp.m.mWaitList.acquireTicks = cputicks()

	// On uniprocessors, no point spinning.
	// On multiprocessors, spin for ACTIVE_SPIN attempts.
	spin := 0
	if ncpu > 1 {
		spin = active_spin
	}
	var enqueued bool
Loop:
	for i := 0; ; i++ {
		v := atomic.Loaduintptr(&l.key)
		if v&mutex_locked == 0 {
			// Unlocked. Try to lock.
			if atomic.Casuintptr(&l.key, v, v|mutex_locked) {
				// We now own the mutex
				v = v | mutex_locked
				for {
					old := v

					head := muintptr(v &^ (mutex_sleeping | mutex_locked))
					fixMutexWaitList(head)
					if enqueued {
						head = removeMutexWaitList(head, gp.m)
					}

					v = mutex_locked
					if head != 0 {
						v = v | uintptr(head) | mutex_sleeping
					}

					if v == old || atomic.Casuintptr(&l.key, old, v) {
						gp.m.mWaitList.clearLinks()
						gp.m.mWaitList.acquireTicks = 0
						break
					}
					v = atomic.Loaduintptr(&l.key)
				}
				if gp == gp.m.curg {
					gp.stackguard0, gp.throwsplit = stackguard0, throwsplit
				}

				if sample {
					endNanos := nanotime()
					gp.m.mLockProfile.waitTime.Add((endNanos - startNanos) * sampleRate)
				}
				return
			}
			i = 0
		}
		if i < spin {
			procyield(active_spin_cnt)
		} else if i < spin+passive_spin {
			osyield()
		} else {
			// Someone else has it.
			// l->key points to a linked list of M's waiting
			// for this lock, chained through m->mWaitList.next.
			// Queue this M.
			for {
				head := v &^ (mutex_locked | mutex_sleeping)
				if !enqueued {
					gp.m.mWaitList.next = muintptr(head)
					head = uintptr(unsafe.Pointer(gp.m))
					if atomic.Casuintptr(&l.key, v, head|mutex_locked|mutex_sleeping) {
						enqueued = true
						break
					}
					gp.m.mWaitList.next = 0
				}
				v = atomic.Loaduintptr(&l.key)
				if v&mutex_locked == 0 {
					continue Loop
				}
			}
			// Queued. Wait.
			futexsleep(key32(&l.key), uint32(v), -1)
			i = 0
		}
	}
}

func unlock(l *mutex) {
	unlockWithRank(l)
}

func unlock2(l *mutex) {
	var claimed bool
	var cycles int64
	for {
		v := atomic.Loaduintptr(&l.key)
		if v == mutex_locked {
			if atomic.Casuintptr(&l.key, mutex_locked, 0) {
				break
			}
		} else if v&mutex_locked == 0 {
			throw("unlock of unlocked lock")
		} else {
			if !claimed {
				claimed = true
				nowTicks := cputicks()
				head := muintptr(v &^ (mutex_sleeping | mutex_locked))
				cycles = claimMutexWaitTime(nowTicks, head)
			}

			// Other M's are waiting for the lock.
			if atomic.Casuintptr(&l.key, v, v&^mutex_locked) {
				futexwakeup(key32(&l.key), 1)
				break
			}
		}
	}

	gp := getg()
	gp.m.mLockProfile.recordUnlock(cycles)
	gp.m.locks--
	if gp.m.locks < 0 {
		throw("runtime·unlock: lock count")
	}
	if gp.m.locks == 0 && gp.preempt { // restore the preemption request in case we've cleared it in newstack
		gp.stackguard0 = stackPreempt
	}
}

// One-time notifications.
func noteclear(n *note) {
	n.key = 0
}

func notewakeup(n *note) {
	old := atomic.Xchg(key32(&n.key), 1)
	if old != 0 {
		print("notewakeup - double wakeup (", old, ")\n")
		throw("notewakeup - double wakeup")
	}
	futexwakeup(key32(&n.key), 1)
}

func notesleep(n *note) {
	gp := getg()
	if gp != gp.m.g0 {
		throw("notesleep not on g0")
	}
	ns := int64(-1)
	if *cgo_yield != nil {
		// Sleep for an arbitrary-but-moderate interval to poll libc interceptors.
		ns = 10e6
	}
	for atomic.Load(key32(&n.key)) == 0 {
		gp.m.blocked = true
		futexsleep(key32(&n.key), 0, ns)
		if *cgo_yield != nil {
			asmcgocall(*cgo_yield, nil)
		}
		gp.m.blocked = false
	}
}

// May run with m.p==nil if called from notetsleep, so write barriers
// are not allowed.
//
//go:nosplit
//go:nowritebarrier
func notetsleep_internal(n *note, ns int64) bool {
	gp := getg()

	if ns < 0 {
		if *cgo_yield != nil {
			// Sleep for an arbitrary-but-moderate interval to poll libc interceptors.
			ns = 10e6
		}
		for atomic.Load(key32(&n.key)) == 0 {
			gp.m.blocked = true
			futexsleep(key32(&n.key), 0, ns)
			if *cgo_yield != nil {
				asmcgocall(*cgo_yield, nil)
			}
			gp.m.blocked = false
		}
		return true
	}

	if atomic.Load(key32(&n.key)) != 0 {
		return true
	}

	deadline := nanotime() + ns
	for {
		if *cgo_yield != nil && ns > 10e6 {
			ns = 10e6
		}
		gp.m.blocked = true
		futexsleep(key32(&n.key), 0, ns)
		if *cgo_yield != nil {
			asmcgocall(*cgo_yield, nil)
		}
		gp.m.blocked = false
		if atomic.Load(key32(&n.key)) != 0 {
			break
		}
		now := nanotime()
		if now >= deadline {
			break
		}
		ns = deadline - now
	}
	return atomic.Load(key32(&n.key)) != 0
}

func notetsleep(n *note, ns int64) bool {
	gp := getg()
	if gp != gp.m.g0 && gp.m.preemptoff != "" {
		throw("notetsleep not on g0")
	}

	return notetsleep_internal(n, ns)
}

// same as runtime·notetsleep, but called on user g (not g0)
// calls only nosplit functions between entersyscallblock/exitsyscall.
func notetsleepg(n *note, ns int64) bool {
	gp := getg()
	if gp == gp.m.g0 {
		throw("notetsleepg on g0")
	}

	entersyscallblock()
	ok := notetsleep_internal(n, ns)
	exitsyscall()
	return ok
}

func beforeIdle(int64, int64) (*g, bool) {
	return nil, false
}

func checkTimeouts() {}
