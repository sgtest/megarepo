package repos

import (
	"container/heap"
	"context"
	"reflect"
	"testing"
	"time"

	"github.com/davecgh/go-spew/spew"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	gitserverprotocol "github.com/sourcegraph/sourcegraph/pkg/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/mutablelimiter"
)

var defaultTime = time.Date(2000, 1, 1, 1, 1, 1, 1, time.UTC)

func init() {
	timeNow = nil
	notify = nil
	timeAfterFunc = nil
}

func mockTime(t time.Time) {
	timeNow = func() time.Time {
		return t
	}
}

type recording struct {
	notifications       []chan struct{}
	timeAfterFuncDelays []time.Duration
}

func startRecording() (*recording, func()) {
	var r recording

	mockTime(defaultTime)

	notify = func(ch chan struct{}) {
		r.notifications = append(r.notifications, ch)
	}

	timeAfterFunc = func(delay time.Duration, f func()) *time.Timer {
		r.timeAfterFuncDelays = append(r.timeAfterFuncDelays, delay)
		f()
		return nil
	}

	return &r, func() {
		timeNow = nil
		notify = nil
		timeAfterFunc = nil
	}
}

func TestUpdateQueue_enqueue(t *testing.T) {
	a := configuredRepo{name: "a", url: "a.com"}
	b := configuredRepo{name: "b", url: "b.com"}
	c := configuredRepo{name: "c", url: "c.com"}
	d := configuredRepo{name: "d", url: "d.com"}
	e := configuredRepo{name: "e", url: "e.com"}

	type enqueueCall struct {
		repo     configuredRepo
		priority priority
	}

	tests := []struct {
		name                  string
		calls                 []*enqueueCall
		expectedUpdates       []*repoUpdate
		expectedNotifications int
	}{
		{
			name: "enqueue low priority",
			calls: []*enqueueCall{
				{repo: a, priority: priorityLow},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityLow,
					seq:      1,
				},
			},
			expectedNotifications: 1,
		},
		{
			name: "enqueue high priority",
			calls: []*enqueueCall{
				{repo: a, priority: priorityHigh},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityHigh,
					seq:      1,
				},
			},
			expectedNotifications: 1,
		},
		{
			name: "enqueue low b then high a",
			calls: []*enqueueCall{
				{repo: b, priority: priorityLow},
				{repo: a, priority: priorityHigh},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityHigh,
					seq:      2,
				},
				{
					repo:     &b,
					priority: priorityLow,
					seq:      1,
				},
			},
			expectedNotifications: 2,
		},
		{
			name: "enqueue high a then low b",
			calls: []*enqueueCall{
				{repo: a, priority: priorityHigh},
				{repo: b, priority: priorityLow},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityHigh,
					seq:      1,
				},
				{
					repo:     &b,
					priority: priorityLow,
					seq:      2,
				},
			},
			expectedNotifications: 2,
		},
		{
			name: "enqueue low a then low a",
			calls: []*enqueueCall{
				{repo: a, priority: priorityLow},
				{repo: a, priority: priorityLow},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityLow,
					seq:      1,
				},
			},
			expectedNotifications: 1,
		},
		{
			name: "enqueue high a then low a",
			calls: []*enqueueCall{
				{repo: a, priority: priorityHigh},
				{repo: a, priority: priorityLow},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityHigh,
					seq:      1,
				},
			},
			expectedNotifications: 1,
		},
		{
			name: "enqueue low a then high a",
			calls: []*enqueueCall{
				{repo: a, priority: priorityLow},
				{repo: a, priority: priorityHigh},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityHigh,
					seq:      2,
				},
			},
			expectedNotifications: 2,
		},
		{
			name: "heap is fixed when priority is bumped",
			calls: []*enqueueCall{
				{repo: c, priority: priorityLow},
				{repo: d, priority: priorityLow},
				{repo: a, priority: priorityLow},
				{repo: e, priority: priorityLow},
				{repo: b, priority: priorityLow},

				{repo: a, priority: priorityHigh},
				{repo: b, priority: priorityHigh},
				{repo: c, priority: priorityHigh},
				{repo: d, priority: priorityHigh},
				{repo: e, priority: priorityHigh},
			},
			expectedUpdates: []*repoUpdate{
				{
					repo:     &a,
					priority: priorityHigh,
					seq:      6,
				},
				{
					repo:     &b,
					priority: priorityHigh,
					seq:      7,
				},
				{
					repo:     &c,
					priority: priorityHigh,
					seq:      8,
				},
				{
					repo:     &d,
					priority: priorityHigh,
					seq:      9,
				},
				{
					repo:     &e,
					priority: priorityHigh,
					seq:      10,
				},
			},
			expectedNotifications: 10,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()

			for _, call := range test.calls {
				s.updateQueue.enqueue(&call.repo, call.priority)
			}

			verifyQueue(t, s, test.expectedUpdates)

			// Verify notifications.
			expectedRecording := &recording{}
			for i := 0; i < test.expectedNotifications; i++ {
				expectedRecording.notifications = append(expectedRecording.notifications, s.updateQueue.notifyEnqueue)
			}
			if !reflect.DeepEqual(expectedRecording, r) {
				t.Fatalf("\nexpected\n%s\ngot\n%s", spew.Sdump(expectedRecording), spew.Sdump(r))
			}
		})
	}
}

func TestUpdateQueue_remove(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}
	c := &configuredRepo{name: "c", url: "c.com"}

	type removeCall struct {
		repo     *configuredRepo
		updating bool
	}

	tests := []struct {
		name         string
		initialQueue []*repoUpdate
		removeCalls  []*removeCall
		finalQueue   []*repoUpdate
	}{
		{
			name: "remove only",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
			},
			removeCalls: []*removeCall{
				{repo: a},
			},
		},
		{
			name: "remove front",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
			},
			removeCalls: []*removeCall{
				{repo: a},
			},
			finalQueue: []*repoUpdate{
				{repo: b, seq: 2},
			},
		},
		{
			name: "remove back",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
			},
			removeCalls: []*removeCall{
				{repo: b},
			},
			finalQueue: []*repoUpdate{
				{repo: a, seq: 1},
			},
		},
		{
			name: "remove middle",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
				{repo: c, seq: 3},
			},
			removeCalls: []*removeCall{
				{repo: c},
			},
			finalQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
			},
		},
		{
			name: "remove not present",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
			},
			removeCalls: []*removeCall{
				{repo: b},
			},
			finalQueue: []*repoUpdate{
				{repo: a, seq: 1},
			},
		},
		{
			name: "remove from empty queue",
			removeCalls: []*removeCall{
				{repo: a},
			},
		},
		{
			name: "remove all",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
				{repo: c, seq: 3},
			},
			removeCalls: []*removeCall{
				{repo: a},
				{repo: b},
				{repo: c},
			},
		},
		{
			name: "remove all reverse",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
				{repo: c, seq: 3},
			},
			removeCalls: []*removeCall{
				{repo: c},
				{repo: b},
				{repo: a},
			},
		},
		{
			name: "don't remove updating",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1, updating: true},
			},
			removeCalls: []*removeCall{
				{repo: a, updating: false},
			},
			finalQueue: []*repoUpdate{
				{repo: a, seq: 1, updating: true},
			},
		},
		{
			name: "don't remove not updating",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1, updating: false},
			},
			removeCalls: []*removeCall{
				{repo: a, updating: true},
			},
			finalQueue: []*repoUpdate{
				{repo: a, seq: 1, updating: false},
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()
			setupInitialQueue(s, test.initialQueue)

			// Perform the removals.
			for _, call := range test.removeCalls {
				s.updateQueue.remove(call.repo, call.updating)
			}

			verifyQueue(t, s, test.finalQueue)

			// Verify no notifications.
			expectedRecording := &recording{}
			if !reflect.DeepEqual(expectedRecording, r) {
				t.Fatalf("\nexpected\n%s\ngot\n%s", spew.Sdump(expectedRecording), spew.Sdump(r))
			}
		})
	}
}

func TestUpdateQueue_acquireNext(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}

	tests := []struct {
		name           string
		initialQueue   []*repoUpdate
		acquireResults []*configuredRepo
		finalQueue     []*repoUpdate
	}{
		{
			name:           "acquire from empty queue returns nil",
			acquireResults: []*configuredRepo{nil},
		},
		{
			name: "acquire sets updating to true",
			initialQueue: []*repoUpdate{
				{repo: a, updating: false, seq: 1},
			},
			acquireResults: []*configuredRepo{a},
			finalQueue: []*repoUpdate{
				{repo: a, updating: true, seq: 1},
			},
		},
		{
			name: "acquire sends update to back of queue",
			initialQueue: []*repoUpdate{
				{repo: a, updating: false, seq: 1},
				{repo: b, updating: false, seq: 2},
			},
			acquireResults: []*configuredRepo{a},
			finalQueue: []*repoUpdate{
				{repo: b, updating: false, seq: 2},
				{repo: a, updating: true, seq: 1},
			},
		},
		{
			name: "acquire does not return repos that are already updating",
			initialQueue: []*repoUpdate{
				{repo: a, updating: true, seq: 1},
			},
			acquireResults: []*configuredRepo{nil},
			finalQueue: []*repoUpdate{
				{repo: a, updating: true, seq: 1},
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()
			setupInitialQueue(s, test.initialQueue)

			// Test aquireNext.
			for i, expected := range test.acquireResults {
				if actual := s.updateQueue.acquireNext(); !reflect.DeepEqual(expected, actual) {
					t.Fatalf("\nacquireNext expected %d\n%s\ngot\n%s", i, spew.Sdump(expected), spew.Sdump(actual))
				}
			}

			verifyQueue(t, s, test.finalQueue)

			// Verify no notifications.
			expectedRecording := &recording{}
			if !reflect.DeepEqual(expectedRecording, r) {
				t.Fatalf("\nexpected\n%s\ngot\n%s", spew.Sdump(expectedRecording), spew.Sdump(r))
			}
		})
	}
}

func setupInitialQueue(s *updateScheduler, initialQueue []*repoUpdate) {
	for _, update := range initialQueue {
		heap.Push(s.updateQueue, update)
	}
}

func verifyQueue(t *testing.T, s *updateScheduler, expected []*repoUpdate) {
	t.Helper()

	var actualQueue []*repoUpdate
	for len(s.updateQueue.heap) > 0 {
		update := heap.Pop(s.updateQueue).(*repoUpdate)
		update.index = 0 // this will always be -1, but easier to set it to 0 to avoid boilerplate in test cases
		actualQueue = append(actualQueue, update)
	}

	if !reflect.DeepEqual(expected, actualQueue) {
		t.Fatalf("\nexpected final queue\n%s\ngot\n%s", spew.Sdump(expected), spew.Sdump(actualQueue))
	}
}

func TestSchedule_add(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}

	type addCall struct {
		time time.Time
		repo *configuredRepo
	}

	tests := []struct {
		name                string
		initialSchedule     []*scheduledRepoUpdate
		addCalls            []*addCall
		finalSchedule       []*scheduledRepoUpdate
		timeAfterFuncDelays []time.Duration
		wakeupNotifications int
	}{
		{
			name: "add to empty schedule",
			addCalls: []*addCall{
				{repo: a, time: defaultTime},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime.Add(minDelay),
					repo:     a,
				},
			},
			timeAfterFuncDelays: []time.Duration{minDelay},
			wakeupNotifications: 1,
		},
		{
			name: "add duplicate is no-op",
			initialSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime,
					repo:     a,
				},
			},
			addCalls: []*addCall{
				{repo: a, time: defaultTime.Add(time.Minute)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime,
					repo:     a,
				},
			},
		},
		{
			name: "add later",
			initialSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime.Add(30 * time.Second),
					repo:     a,
				},
			},
			addCalls: []*addCall{
				{repo: b, time: defaultTime.Add(time.Second)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime.Add(30 * time.Second),
					repo:     a,
				},
				{
					interval: minDelay,
					due:      defaultTime.Add(time.Second + minDelay),
					repo:     b,
				},
			},
			timeAfterFuncDelays: []time.Duration{29 * time.Second},
			wakeupNotifications: 1,
		},
		{
			name: "add before",
			initialSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime.Add(time.Minute),
					repo:     a,
				},
			},
			addCalls: []*addCall{
				{repo: b, time: defaultTime.Add(time.Second)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					interval: minDelay,
					due:      defaultTime.Add(time.Second + minDelay),
					repo:     b,
				},
				{
					interval: minDelay,
					due:      defaultTime.Add(time.Minute),
					repo:     a,
				},
			},
			timeAfterFuncDelays: []time.Duration{minDelay},
			wakeupNotifications: 1,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()
			setupInitialSchedule(s, test.initialSchedule)

			for _, call := range test.addCalls {
				mockTime(call.time)
				s.schedule.add(call.repo)
			}

			verifySchedule(t, s, test.finalSchedule)
			verifyScheduleRecording(t, s, test.timeAfterFuncDelays, test.wakeupNotifications, r)
		})
	}
}

func TestSchedule_updateInterval(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}
	c := &configuredRepo{name: "c", url: "c.com"}
	d := &configuredRepo{name: "d", url: "d.com"}
	e := &configuredRepo{name: "e", url: "e.com"}

	type updateCall struct {
		time     time.Time
		repo     *configuredRepo
		interval time.Duration
	}

	tests := []struct {
		name                string
		initialSchedule     []*scheduledRepoUpdate
		updateCalls         []*updateCall
		finalSchedule       []*scheduledRepoUpdate
		timeAfterFuncDelays []time.Duration
		wakeupNotifications int
	}{
		{
			name: "update has no effect if repo isn't in schedule",
			updateCalls: []*updateCall{
				{repo: a, time: defaultTime},
			},
		},
		{
			name: "update earlier",
			initialSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: minDelay,
					due:      defaultTime.Add(time.Hour),
				},
			},
			updateCalls: []*updateCall{
				{
					repo:     a,
					time:     defaultTime.Add(time.Second),
					interval: 123 * time.Second,
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: 123 * time.Second,
					due:      defaultTime.Add(124 * time.Second),
				},
			},
			timeAfterFuncDelays: []time.Duration{123 * time.Second},
			wakeupNotifications: 1,
		},
		{
			name: "minimum interval",
			initialSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: maxDelay,
					due:      defaultTime.Add(maxDelay),
				},
			},
			updateCalls: []*updateCall{
				{
					repo:     a,
					time:     defaultTime,
					interval: time.Second,
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: minDelay,
					due:      defaultTime.Add(minDelay),
				},
			},
			timeAfterFuncDelays: []time.Duration{minDelay},
			wakeupNotifications: 1,
		},
		{
			name: "maximum interval",
			initialSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: minDelay,
					due:      defaultTime.Add(minDelay),
				},
			},
			updateCalls: []*updateCall{
				{
					repo:     a,
					time:     defaultTime,
					interval: 365 * 25 * time.Hour,
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: maxDelay,
					due:      defaultTime.Add(maxDelay),
				},
			},
			timeAfterFuncDelays: []time.Duration{maxDelay},
			wakeupNotifications: 1,
		},
		{
			name: "update later",
			initialSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: minDelay,
					due:      defaultTime.Add(time.Hour),
				},
			},
			updateCalls: []*updateCall{
				{
					repo:     a,
					time:     defaultTime.Add(time.Second),
					interval: 123 * time.Minute,
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{
					repo:     a,
					interval: 123 * time.Minute,
					due:      defaultTime.Add(time.Second + 123*time.Minute),
				},
			},
			timeAfterFuncDelays: []time.Duration{123 * time.Minute},
			wakeupNotifications: 1,
		},
		{
			name: "heap reorders correctly",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: c, interval: minDelay, due: defaultTime.Add(1 * time.Minute)},
				{repo: d, interval: minDelay, due: defaultTime.Add(2 * time.Minute)},
				{repo: a, interval: minDelay, due: defaultTime.Add(3 * time.Minute)},
				{repo: e, interval: minDelay, due: defaultTime.Add(4 * time.Minute)},
				{repo: b, interval: minDelay, due: defaultTime.Add(5 * time.Minute)},
			},
			updateCalls: []*updateCall{
				{repo: a, time: defaultTime, interval: 1 * time.Minute},
				{repo: b, time: defaultTime, interval: 2 * time.Minute},
				{repo: c, time: defaultTime, interval: 3 * time.Minute},
				{repo: d, time: defaultTime, interval: 4 * time.Minute},
				{repo: e, time: defaultTime, interval: 5 * time.Minute},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 1 * time.Minute, due: defaultTime.Add(1 * time.Minute)},
				{repo: b, interval: 2 * time.Minute, due: defaultTime.Add(2 * time.Minute)},
				{repo: c, interval: 3 * time.Minute, due: defaultTime.Add(3 * time.Minute)},
				{repo: d, interval: 4 * time.Minute, due: defaultTime.Add(4 * time.Minute)},
				{repo: e, interval: 5 * time.Minute, due: defaultTime.Add(5 * time.Minute)},
			},
			timeAfterFuncDelays: []time.Duration{time.Minute, time.Minute, time.Minute, time.Minute, time.Minute},
			wakeupNotifications: 5,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()
			setupInitialSchedule(s, test.initialSchedule)

			for _, call := range test.updateCalls {
				mockTime(call.time)
				s.schedule.updateInterval(call.repo, call.interval)
			}

			verifySchedule(t, s, test.finalSchedule)
			verifyScheduleRecording(t, s, test.timeAfterFuncDelays, test.wakeupNotifications, r)
		})
	}
}

func TestSchedule_remove(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}
	c := &configuredRepo{name: "c", url: "c.com"}

	type removeCall struct {
		time time.Time
		repo *configuredRepo
	}

	tests := []struct {
		name                string
		initialSchedule     []*scheduledRepoUpdate
		removeCalls         []*removeCall
		finalSchedule       []*scheduledRepoUpdate
		timeAfterFuncDelays []time.Duration
		wakeupNotifications int
	}{
		{
			name: "remove on empty schedule",
			removeCalls: []*removeCall{
				{repo: a, time: defaultTime},
			},
		},
		{
			name: "remove has no effect if repo isn't in schedule",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a},
			},
			removeCalls: []*removeCall{
				{repo: b},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a},
			},
		},
		{
			name: "remove last scheduled doesn't reschedule timer",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a},
			},
			removeCalls: []*removeCall{
				{repo: a},
			},
		},
		{
			name: "remove next reschedules timer",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: minDelay, due: defaultTime},
				{repo: b, interval: minDelay, due: defaultTime.Add(minDelay)},
				{repo: c, interval: maxDelay, due: defaultTime.Add(maxDelay)},
			},
			removeCalls: []*removeCall{
				{repo: a, time: defaultTime},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: b, interval: minDelay, due: defaultTime.Add(minDelay)},
				{repo: c, interval: maxDelay, due: defaultTime.Add(maxDelay)},
			},
			timeAfterFuncDelays: []time.Duration{minDelay},
			wakeupNotifications: 1,
		},
		{
			name: "remove not-next doesn't reschedule timer",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: minDelay, due: defaultTime},
				{repo: b, interval: minDelay, due: defaultTime.Add(minDelay)},
				{repo: c, interval: maxDelay, due: defaultTime.Add(maxDelay)},
			},
			removeCalls: []*removeCall{
				{repo: b, time: defaultTime},
				{repo: c, time: defaultTime},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: minDelay, due: defaultTime},
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()
			setupInitialSchedule(s, test.initialSchedule)

			for _, call := range test.removeCalls {
				mockTime(call.time)
				s.schedule.remove(call.repo)
			}

			verifySchedule(t, s, test.finalSchedule)
			verifyScheduleRecording(t, s, test.timeAfterFuncDelays, test.wakeupNotifications, r)
		})
	}
}

func setupInitialSchedule(s *updateScheduler, initialSchedule []*scheduledRepoUpdate) {
	for _, update := range initialSchedule {
		heap.Push(s.schedule, update)
	}
}

func verifySchedule(t *testing.T, s *updateScheduler, expected []*scheduledRepoUpdate) {
	t.Helper()

	var actualSchedule []*scheduledRepoUpdate
	for len(s.schedule.heap) > 0 {
		update := heap.Pop(s.schedule).(*scheduledRepoUpdate)
		update.index = 0 // this will always be -1, but easier to set it to 0 to avoid boilerplate in test cases
		actualSchedule = append(actualSchedule, update)
	}

	if !reflect.DeepEqual(expected, actualSchedule) {
		t.Fatalf("\nexpected final schedule\n%s\ngot\n%s", spew.Sdump(expected), spew.Sdump(actualSchedule))
	}
}

func verifyScheduleRecording(t *testing.T, s *updateScheduler, timeAfterFuncDelays []time.Duration, wakeupNotifications int, r *recording) {
	t.Helper()

	if !reflect.DeepEqual(timeAfterFuncDelays, r.timeAfterFuncDelays) {
		t.Fatalf("\nexpected timeAfterFuncDelays\n%s\ngot\n%s", spew.Sdump(timeAfterFuncDelays), spew.Sdump(r.timeAfterFuncDelays))
	}

	if l := len(r.notifications); l != wakeupNotifications {
		t.Fatalf("expected %d notifications; got %d", wakeupNotifications, l)
	}

	for _, n := range r.notifications {
		if n != s.schedule.wakeup {
			t.Fatalf("received notification on wrong channel")
		}
	}
}

func TestUpdateScheduler_runScheduleLoop(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}
	c := &configuredRepo{name: "c", url: "c.com"}
	d := &configuredRepo{name: "d", url: "d.com"}
	e := &configuredRepo{name: "e", url: "e.com"}

	tests := []struct {
		name                  string
		initialSchedule       []*scheduledRepoUpdate
		finalSchedule         []*scheduledRepoUpdate
		finalQueue            []*repoUpdate
		timeAfterFuncDelays   []time.Duration
		expectedNotifications func(s *updateScheduler) []chan struct{}
	}{
		{
			name: "empty schedule",
		},
		{
			name: "no updates due",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 11 * time.Second, due: defaultTime.Add(time.Minute)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 11 * time.Second, due: defaultTime.Add(time.Minute)},
			},
			timeAfterFuncDelays: []time.Duration{time.Minute},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{s.schedule.wakeup}
			},
		},
		{
			name: "one update due, rescheduled to front",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 11 * time.Second, due: defaultTime.Add(1 * time.Microsecond)},
				{repo: b, interval: 22 * time.Second, due: defaultTime.Add(time.Minute)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 11 * time.Second, due: defaultTime.Add(11 * time.Second)},
				{repo: b, interval: 22 * time.Second, due: defaultTime.Add(time.Minute)},
			},
			finalQueue: []*repoUpdate{
				{repo: a, priority: priorityLow, seq: 1},
			},
			timeAfterFuncDelays: []time.Duration{11 * time.Second},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{s.updateQueue.notifyEnqueue, s.schedule.wakeup}
			},
		},
		{
			name: "one update due, rescheduled to back",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 11 * time.Minute, due: defaultTime},
				{repo: b, interval: 22 * time.Second, due: defaultTime.Add(time.Minute)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: b, interval: 22 * time.Second, due: defaultTime.Add(time.Minute)},
				{repo: a, interval: 11 * time.Minute, due: defaultTime.Add(11 * time.Minute)},
			},
			finalQueue: []*repoUpdate{
				{repo: a, priority: priorityLow, seq: 1},
			},
			timeAfterFuncDelays: []time.Duration{time.Minute},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{s.updateQueue.notifyEnqueue, s.schedule.wakeup}
			},
		},
		{
			name: "all updates due",
			initialSchedule: []*scheduledRepoUpdate{
				{repo: c, interval: 3 * time.Minute, due: defaultTime.Add(-5 * time.Minute)},
				{repo: d, interval: 4 * time.Minute, due: defaultTime.Add(-4 * time.Minute)},
				{repo: a, interval: 1 * time.Minute, due: defaultTime.Add(-3 * time.Minute)},
				{repo: e, interval: 5 * time.Minute, due: defaultTime.Add(-2 * time.Minute)},
				{repo: b, interval: 2 * time.Minute, due: defaultTime.Add(-1 * time.Minute)},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: 1 * time.Minute, due: defaultTime.Add(1 * time.Minute)},
				{repo: b, interval: 2 * time.Minute, due: defaultTime.Add(2 * time.Minute)},
				{repo: c, interval: 3 * time.Minute, due: defaultTime.Add(3 * time.Minute)},
				{repo: d, interval: 4 * time.Minute, due: defaultTime.Add(4 * time.Minute)},
				{repo: e, interval: 5 * time.Minute, due: defaultTime.Add(5 * time.Minute)},
			},
			finalQueue: []*repoUpdate{
				{repo: c, priority: priorityLow, seq: 1},
				{repo: d, priority: priorityLow, seq: 2},
				{repo: a, priority: priorityLow, seq: 3},
				{repo: e, priority: priorityLow, seq: 4},
				{repo: b, priority: priorityLow, seq: 5},
			},
			timeAfterFuncDelays: []time.Duration{1 * time.Minute},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{
					s.updateQueue.notifyEnqueue,
					s.updateQueue.notifyEnqueue,
					s.updateQueue.notifyEnqueue,
					s.updateQueue.notifyEnqueue,
					s.updateQueue.notifyEnqueue,
					s.schedule.wakeup,
				}
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()

			// unbuffer the wakeup channel
			s.schedule.wakeup = make(chan struct{})

			setupInitialSchedule(s, test.initialSchedule)

			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()

			done := make(chan struct{})
			go func() {
				s.runScheduleLoop(ctx)
				close(done)
			}()

			// Let the goroutine do a single loop.
			s.schedule.wakeup <- struct{}{}

			// Cancel after the first loop.
			// This doesn't race with the wakeup notification because the channel is not buffered.
			cancel()

			// Wait for the goroutine to exit so we can verify the final state.
			<-done

			verifySchedule(t, s, test.finalSchedule)
			verifyQueue(t, s, test.finalQueue)
			verifyRecording(t, s, test.timeAfterFuncDelays, test.expectedNotifications, r)
		})
	}
}

func TestUpdateScheduler_runUpdateLoop(t *testing.T) {
	a := &configuredRepo{name: "a", url: "a.com"}
	b := &configuredRepo{name: "b", url: "b.com"}
	c := &configuredRepo{name: "c", url: "c.com"}

	type mockRequestRepoUpdate struct {
		repo *configuredRepo
		resp *gitserverprotocol.RepoUpdateResponse
		err  error
	}

	tests := []struct {
		name                   string
		gitMaxConcurrentClones int
		initialSchedule        []*scheduledRepoUpdate
		initialQueue           []*repoUpdate
		mockRequestRepoUpdates []*mockRequestRepoUpdate
		finalSchedule          []*scheduledRepoUpdate
		finalQueue             []*repoUpdate
		timeAfterFuncDelays    []time.Duration
		expectedNotifications  func(s *updateScheduler) []chan struct{}
	}{
		{
			name: "empty queue",
		},
		{
			name: "non-empty queue at clone limit",
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
			},
			finalQueue: []*repoUpdate{
				{repo: a, seq: 1},
			},
		},
		{
			name:                   "queue drains",
			gitMaxConcurrentClones: 1,
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 2},
				{repo: c, seq: 3},
			},
			mockRequestRepoUpdates: []*mockRequestRepoUpdate{
				{repo: a},
				{repo: b},
				{repo: c},
			},
		},
		{
			name:                   "schedule updated",
			gitMaxConcurrentClones: 1,
			initialSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: time.Hour, due: defaultTime.Add(time.Hour)},
			},
			initialQueue: []*repoUpdate{
				{repo: a, seq: 1},
				{repo: b, seq: 1},
			},
			mockRequestRepoUpdates: []*mockRequestRepoUpdate{
				{
					repo: a,
					resp: &gitserverprotocol.RepoUpdateResponse{
						LastFetched: timePtr(defaultTime.Add(2 * time.Minute)),
						LastChanged: timePtr(defaultTime),
					},
				},
				{
					repo: b,
					resp: &gitserverprotocol.RepoUpdateResponse{
						LastFetched: timePtr(defaultTime.Add(2 * time.Minute)),
						LastChanged: timePtr(defaultTime),
					},
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: a, interval: time.Minute, due: defaultTime.Add(time.Minute)},
			},
			timeAfterFuncDelays: []time.Duration{time.Minute},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{s.schedule.wakeup}
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			configuredLimiter = func() *mutablelimiter.Limiter {
				return mutablelimiter.New(test.gitMaxConcurrentClones)
			}
			defer func() {
				configuredLimiter = nil
			}()

			expectedRequestCount := len(test.mockRequestRepoUpdates)
			mockRequestRepoUpdates := make(chan *mockRequestRepoUpdate, expectedRequestCount)
			for _, m := range test.mockRequestRepoUpdates {
				mockRequestRepoUpdates <- m
			}
			// intentionally don't close the channel so any futher receives just block

			contexts := make(chan context.Context, expectedRequestCount)
			requestRepoUpdate = func(ctx context.Context, repo *configuredRepo, since time.Duration) (*gitserverprotocol.RepoUpdateResponse, error) {
				select {
				case mock := <-mockRequestRepoUpdates:
					if !reflect.DeepEqual(mock.repo, repo) {
						t.Errorf("\nexpected requestRepoUpdate\n%s\ngot\n%s", spew.Sdump(mock.repo), spew.Sdump(repo))
					}
					contexts <- ctx // Intercept all contexts so we can wait for spawned goroutines to finish.
					return mock.resp, mock.err
				case <-ctx.Done():
					return nil, ctx.Err()
				}
			}
			defer func() { requestRepoUpdate = nil }()

			s := newUpdateScheduler()

			// unbuffer the channel
			s.updateQueue.notifyEnqueue = make(chan struct{})

			setupInitialSchedule(s, test.initialSchedule)
			setupInitialQueue(s, test.initialQueue)

			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()

			done := make(chan struct{})
			go func() {
				s.runUpdateLoop(ctx)
				close(done)
			}()

			// Let the goroutine do a single loop.
			s.updateQueue.notifyEnqueue <- struct{}{}

			// Wait for all goroutines that have a mock request to finish.
			// There may be additional goroutines which don't have a mock request
			// and will block until the context is canceled.
			for i := 0; i < expectedRequestCount; i++ {
				ctx := <-contexts
				<-ctx.Done()
			}

			verifySchedule(t, s, test.finalSchedule)
			verifyQueue(t, s, test.finalQueue)
			verifyRecording(t, s, test.timeAfterFuncDelays, test.expectedNotifications, r)

			// Cancel the context.
			cancel()

			// Wait for the goroutine to exit.
			<-done
		})
	}
}

func verifyRecording(t *testing.T, s *updateScheduler, timeAfterFuncDelays []time.Duration, expectedNotifications func(s *updateScheduler) []chan struct{}, r *recording) {
	if !reflect.DeepEqual(timeAfterFuncDelays, r.timeAfterFuncDelays) {
		t.Fatalf("\nexpected timeAfterFuncDelays\n%s\ngot\n%s", spew.Sdump(timeAfterFuncDelays), spew.Sdump(r.timeAfterFuncDelays))
	}

	if expectedNotifications == nil {
		expectedNotifications = func(s *updateScheduler) []chan struct{} {
			return nil
		}
	}

	if expected := expectedNotifications(s); !reflect.DeepEqual(expected, r.notifications) {
		t.Fatalf("\nexpected notifications\n%s\ngot\n%s", spew.Sdump(expected), spew.Sdump(r.notifications))
	}
}

func timePtr(t time.Time) *time.Time {
	return &t
}

func TestUpdateScheduler_updateSource(t *testing.T) {
	type updateSourceCall struct {
		source  string
		newList sourceRepoMap
	}

	tests := []struct {
		name                  string
		initialSourceRepos    map[string]sourceRepoMap
		initialSchedule       []*scheduledRepoUpdate
		initialQueue          []*repoUpdate
		updateSourceCalls     []*updateSourceCall
		finalSourceRepos      map[string]sourceRepoMap
		finalSchedule         []*scheduledRepoUpdate
		finalQueue            []*repoUpdate
		timeAfterFuncDelays   []time.Duration
		expectedNotifications func(s *updateScheduler) []chan struct{}
	}{
		{
			name:               "add disabled repo",
			initialSourceRepos: map[string]sourceRepoMap{},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: false},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: false},
				},
			},
		},
		{
			name:               "add enabled repo",
			initialSourceRepos: map[string]sourceRepoMap{},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime.Add(minDelay)},
			},
			finalQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, seq: 1, updating: false},
			},
			timeAfterFuncDelays: []time.Duration{minDelay},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{s.schedule.wakeup, s.updateQueue.notifyEnqueue}
			},
		},
		{
			name: "update disabled repo",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: false},
				},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: true},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: true},
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "aa.com", enabled: true}, interval: minDelay, due: defaultTime.Add(minDelay)},
			},
			finalQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "aa.com", enabled: true}, seq: 1, updating: false},
			},
			timeAfterFuncDelays: []time.Duration{minDelay},
			expectedNotifications: func(s *updateScheduler) []chan struct{} {
				return []chan struct{}{s.schedule.wakeup, s.updateQueue.notifyEnqueue}
			},
		},
		{
			name: "disabled repo removed from schedule and queue",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			initialSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime},
			},
			initialQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: false}, updating: false},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: false},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: false},
				},
			},
		},
		{
			name: "missing repo removed from schedule and queue",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			initialSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime},
			},
			initialQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true /* enabled state doesn't get updated once in the queue because concurrency nightmare */}, updating: false},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source:  "a",
					newList: sourceRepoMap{},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{},
			},
		},
		{
			name: "disabled repo not removed from queue when updating",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			initialSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime},
			},
			initialQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, updating: true},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: false},
					},
				},
			},
			finalQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, seq: 1, updating: true},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: false},
				},
			},
		},
		{
			name: "missing repo not removed from queue when updating",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			initialSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime},
			},
			initialQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, updating: true},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source:  "a",
					newList: sourceRepoMap{},
				},
			},
			finalQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, seq: 1, updating: true},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{},
			},
		},
		{
			name: "enabled repo updated",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			initialSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime.Add(minDelay)},
			},
			initialQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, seq: 1, updating: false},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: true},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: true},
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "aa.com", enabled: true}, interval: minDelay, due: defaultTime.Add(minDelay)},
			},
			finalQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "aa.com", enabled: true}, seq: 1, updating: false},
			},
		},
		{
			name: "disabled repo updated",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: false},
				},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: false},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: false},
				},
			},
		},
		{
			name: "update enabled repo while updating",
			initialSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "a.com", enabled: true},
				},
			},
			initialSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, interval: minDelay, due: defaultTime.Add(minDelay)},
			},
			initialQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, seq: 1, updating: true},
			},
			updateSourceCalls: []*updateSourceCall{
				{
					source: "a",
					newList: sourceRepoMap{
						api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: true},
					},
				},
			},
			finalSourceRepos: map[string]sourceRepoMap{
				"a": sourceRepoMap{
					api.RepoName("a/a"): &configuredRepo{name: "a", url: "aa.com", enabled: true},
				},
			},
			finalSchedule: []*scheduledRepoUpdate{
				{repo: &configuredRepo{name: "a", url: "aa.com", enabled: true}, interval: minDelay, due: defaultTime.Add(minDelay)},
			},
			finalQueue: []*repoUpdate{
				{repo: &configuredRepo{name: "a", url: "a.com", enabled: true}, seq: 1, updating: true},
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			r, stop := startRecording()
			defer stop()

			s := newUpdateScheduler()
			s.sourceRepos = test.initialSourceRepos
			setupInitialSchedule(s, test.initialSchedule)
			setupInitialQueue(s, test.initialQueue)

			for _, call := range test.updateSourceCalls {
				s.updateSource(call.source, call.newList)
			}

			if !reflect.DeepEqual(s.sourceRepos, test.finalSourceRepos) {
				t.Fatalf("\nexpected source repos\n%s\ngot\n%s", spew.Sdump(test.finalSourceRepos), spew.Sdump(s.sourceRepos))
			}

			verifySchedule(t, s, test.finalSchedule)
			verifyQueue(t, s, test.finalQueue)
			verifyRecording(t, s, test.timeAfterFuncDelays, test.expectedNotifications, r)
		})
	}
}

// TODO: update enabled state and url once in the queue?
