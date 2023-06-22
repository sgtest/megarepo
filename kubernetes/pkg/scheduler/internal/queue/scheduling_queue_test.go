/*
Copyright 2017 The Kubernetes Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package queue

import (
	"context"
	"fmt"
	"math"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	v1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/apimachinery/pkg/util/sets"
	"k8s.io/client-go/informers"
	"k8s.io/client-go/kubernetes/fake"
	"k8s.io/component-base/metrics/testutil"
	"k8s.io/klog/v2"
	"k8s.io/klog/v2/ktesting"
	podutil "k8s.io/kubernetes/pkg/api/v1/pod"
	"k8s.io/kubernetes/pkg/scheduler/framework"
	"k8s.io/kubernetes/pkg/scheduler/framework/plugins/queuesort"
	"k8s.io/kubernetes/pkg/scheduler/metrics"
	st "k8s.io/kubernetes/pkg/scheduler/testing"
	"k8s.io/kubernetes/pkg/scheduler/util"
	testingclock "k8s.io/utils/clock/testing"
)

const queueMetricMetadata = `
		# HELP scheduler_queue_incoming_pods_total [STABLE] Number of pods added to scheduling queues by event and queue type.
		# TYPE scheduler_queue_incoming_pods_total counter
	`

var (
	TestEvent    = framework.ClusterEvent{Resource: "test"}
	NodeAllEvent = framework.ClusterEvent{Resource: framework.Node, ActionType: framework.All}
	EmptyEvent   = framework.ClusterEvent{}

	lowPriority, midPriority, highPriority = int32(0), int32(100), int32(1000)
	mediumPriority                         = (lowPriority + highPriority) / 2

	highPriorityPodInfo = mustNewPodInfo(
		st.MakePod().Name("hpp").Namespace("ns1").UID("hppns1").Priority(highPriority).Obj(),
	)
	highPriNominatedPodInfo = mustNewPodInfo(
		st.MakePod().Name("hpp").Namespace("ns1").UID("hppns1").Priority(highPriority).NominatedNodeName("node1").Obj(),
	)
	medPriorityPodInfo = mustNewPodInfo(
		st.MakePod().Name("mpp").Namespace("ns2").UID("mppns2").Annotation("annot2", "val2").Priority(mediumPriority).NominatedNodeName("node1").Obj(),
	)
	unschedulablePodInfo = mustNewPodInfo(
		st.MakePod().Name("up").Namespace("ns1").UID("upns1").Annotation("annot2", "val2").Priority(lowPriority).NominatedNodeName("node1").Condition(v1.PodScheduled, v1.ConditionFalse, v1.PodReasonUnschedulable).Obj(),
	)
	nonExistentPodInfo = mustNewPodInfo(
		st.MakePod().Name("ne").Namespace("ns1").UID("nens1").Obj(),
	)
	scheduledPodInfo = mustNewPodInfo(
		st.MakePod().Name("sp").Namespace("ns1").UID("spns1").Node("foo").Obj(),
	)

	nominatorCmpOpts = []cmp.Option{
		cmp.AllowUnexported(nominator{}),
		cmpopts.IgnoreFields(nominator{}, "podLister", "lock"),
	}
)

func getUnschedulablePod(p *PriorityQueue, pod *v1.Pod) *v1.Pod {
	pInfo := p.unschedulablePods.get(pod)
	if pInfo != nil {
		return pInfo.Pod
	}
	return nil
}

func TestPriorityQueue_Add(t *testing.T) {
	objs := []runtime.Object{medPriorityPodInfo.Pod, unschedulablePodInfo.Pod, highPriorityPodInfo.Pod}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	if err := q.Add(logger, medPriorityPodInfo.Pod); err != nil {
		t.Errorf("add failed: %v", err)
	}
	if err := q.Add(logger, unschedulablePodInfo.Pod); err != nil {
		t.Errorf("add failed: %v", err)
	}
	if err := q.Add(logger, highPriorityPodInfo.Pod); err != nil {
		t.Errorf("add failed: %v", err)
	}
	expectedNominatedPods := &nominator{
		nominatedPodToNode: map[types.UID]string{
			medPriorityPodInfo.Pod.UID:   "node1",
			unschedulablePodInfo.Pod.UID: "node1",
		},
		nominatedPods: map[string][]*framework.PodInfo{
			"node1": {medPriorityPodInfo, unschedulablePodInfo},
		},
	}
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after adding pods (-want, +got):\n%s", diff)
	}
	if p, err := q.Pop(); err != nil || p.Pod != highPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", highPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
	if p, err := q.Pop(); err != nil || p.Pod != medPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", medPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
	if p, err := q.Pop(); err != nil || p.Pod != unschedulablePodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", unschedulablePodInfo.Pod.Name, p.Pod.Name)
	}
	if len(q.nominator.nominatedPods["node1"]) != 2 {
		t.Errorf("Expected medPriorityPodInfo and unschedulablePodInfo to be still present in nomindatePods: %v", q.nominator.nominatedPods["node1"])
	}
}

func newDefaultQueueSort() framework.LessFunc {
	sort := &queuesort.PrioritySort{}
	return sort.Less
}

func TestPriorityQueue_AddWithReversePriorityLessFunc(t *testing.T) {
	objs := []runtime.Object{medPriorityPodInfo.Pod, highPriorityPodInfo.Pod}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	if err := q.Add(logger, medPriorityPodInfo.Pod); err != nil {
		t.Errorf("add failed: %v", err)
	}
	if err := q.Add(logger, highPriorityPodInfo.Pod); err != nil {
		t.Errorf("add failed: %v", err)
	}
	if p, err := q.Pop(); err != nil || p.Pod != highPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", highPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
	if p, err := q.Pop(); err != nil || p.Pod != medPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", medPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
}

func TestPriorityQueue_AddUnschedulableIfNotPresent(t *testing.T) {
	objs := []runtime.Object{highPriNominatedPodInfo.Pod, unschedulablePodInfo.Pod}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	q.Add(logger, highPriNominatedPodInfo.Pod)
	q.AddUnschedulableIfNotPresent(logger, newQueuedPodInfoForLookup(highPriNominatedPodInfo.Pod), q.SchedulingCycle()) // Must not add anything.
	q.AddUnschedulableIfNotPresent(logger, newQueuedPodInfoForLookup(unschedulablePodInfo.Pod), q.SchedulingCycle())
	expectedNominatedPods := &nominator{
		nominatedPodToNode: map[types.UID]string{
			unschedulablePodInfo.Pod.UID:    "node1",
			highPriNominatedPodInfo.Pod.UID: "node1",
		},
		nominatedPods: map[string][]*framework.PodInfo{
			"node1": {highPriNominatedPodInfo, unschedulablePodInfo},
		},
	}
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after adding pods (-want, +got):\n%s", diff)
	}
	if p, err := q.Pop(); err != nil || p.Pod != highPriNominatedPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", highPriNominatedPodInfo.Pod.Name, p.Pod.Name)
	}
	if len(q.nominator.nominatedPods) != 1 {
		t.Errorf("Expected nomindatePods to have one element: %v", q.nominator)
	}
	if getUnschedulablePod(q, unschedulablePodInfo.Pod) != unschedulablePodInfo.Pod {
		t.Errorf("Pod %v was not found in the unschedulablePods.", unschedulablePodInfo.Pod.Name)
	}
}

// TestPriorityQueue_AddUnschedulableIfNotPresent_Backoff tests the scenarios when
// AddUnschedulableIfNotPresent is called asynchronously.
// Pods in and before current scheduling cycle will be put back to activeQueue
// if we were trying to schedule them when we received move request.
func TestPriorityQueue_AddUnschedulableIfNotPresent_Backoff(t *testing.T) {
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(testingclock.NewFakeClock(time.Now())))
	totalNum := 10
	expectedPods := make([]v1.Pod, 0, totalNum)
	for i := 0; i < totalNum; i++ {
		priority := int32(i)
		p := st.MakePod().Name(fmt.Sprintf("pod%d", i)).Namespace(fmt.Sprintf("ns%d", i)).UID(fmt.Sprintf("upns%d", i)).Priority(priority).Obj()
		expectedPods = append(expectedPods, *p)
		// priority is to make pods ordered in the PriorityQueue
		q.Add(logger, p)
	}

	// Pop all pods except for the first one
	for i := totalNum - 1; i > 0; i-- {
		p, _ := q.Pop()
		if !reflect.DeepEqual(&expectedPods[i], p.Pod) {
			t.Errorf("Unexpected pod. Expected: %v, got: %v", &expectedPods[i], p)
		}
	}

	// move all pods to active queue when we were trying to schedule them
	q.MoveAllToActiveOrBackoffQueue(logger, TestEvent, nil)
	oldCycle := q.SchedulingCycle()

	firstPod, _ := q.Pop()
	if !reflect.DeepEqual(&expectedPods[0], firstPod.Pod) {
		t.Errorf("Unexpected pod. Expected: %v, got: %v", &expectedPods[0], firstPod)
	}

	// mark pods[1] ~ pods[totalNum-1] as unschedulable and add them back
	for i := 1; i < totalNum; i++ {
		unschedulablePod := expectedPods[i].DeepCopy()
		unschedulablePod.Status = v1.PodStatus{
			Conditions: []v1.PodCondition{
				{
					Type:   v1.PodScheduled,
					Status: v1.ConditionFalse,
					Reason: v1.PodReasonUnschedulable,
				},
			},
		}

		if err := q.AddUnschedulableIfNotPresent(logger, newQueuedPodInfoForLookup(unschedulablePod), oldCycle); err != nil {
			t.Errorf("Failed to call AddUnschedulableIfNotPresent(%v): %v", unschedulablePod.Name, err)
		}
	}

	// Since there was a move request at the same cycle as "oldCycle", these pods
	// should be in the backoff queue.
	for i := 1; i < totalNum; i++ {
		if _, exists, _ := q.podBackoffQ.Get(newQueuedPodInfoForLookup(&expectedPods[i])); !exists {
			t.Errorf("Expected %v to be added to podBackoffQ.", expectedPods[i].Name)
		}
	}
}

func TestPriorityQueue_Pop(t *testing.T) {
	objs := []runtime.Object{medPriorityPodInfo.Pod}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	wg := sync.WaitGroup{}
	wg.Add(1)
	go func() {
		defer wg.Done()
		if p, err := q.Pop(); err != nil || p.Pod != medPriorityPodInfo.Pod {
			t.Errorf("Expected: %v after Pop, but got: %v", medPriorityPodInfo.Pod.Name, p.Pod.Name)
		}
		if len(q.nominator.nominatedPods["node1"]) != 1 {
			t.Errorf("Expected medPriorityPodInfo to be present in nomindatePods: %v", q.nominator.nominatedPods["node1"])
		}
	}()
	q.Add(logger, medPriorityPodInfo.Pod)
	wg.Wait()
}

func TestPriorityQueue_Update(t *testing.T) {
	objs := []runtime.Object{highPriorityPodInfo.Pod, unschedulablePodInfo.Pod, medPriorityPodInfo.Pod}
	c := testingclock.NewFakeClock(time.Now())
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs, WithClock(c))
	q.Update(logger, nil, highPriorityPodInfo.Pod)
	if _, exists, _ := q.activeQ.Get(newQueuedPodInfoForLookup(highPriorityPodInfo.Pod)); !exists {
		t.Errorf("Expected %v to be added to activeQ.", highPriorityPodInfo.Pod.Name)
	}
	if len(q.nominator.nominatedPods) != 0 {
		t.Errorf("Expected nomindatePods to be empty: %v", q.nominator)
	}
	// Update highPriorityPodInfo and add a nominatedNodeName to it.
	q.Update(logger, highPriorityPodInfo.Pod, highPriNominatedPodInfo.Pod)
	if q.activeQ.Len() != 1 {
		t.Error("Expected only one item in activeQ.")
	}
	if len(q.nominator.nominatedPods) != 1 {
		t.Errorf("Expected one item in nomindatePods map: %v", q.nominator)
	}
	// Updating an unschedulable pod which is not in any of the two queues, should
	// add the pod to activeQ.
	q.Update(logger, unschedulablePodInfo.Pod, unschedulablePodInfo.Pod)
	if _, exists, _ := q.activeQ.Get(newQueuedPodInfoForLookup(unschedulablePodInfo.Pod)); !exists {
		t.Errorf("Expected %v to be added to activeQ.", unschedulablePodInfo.Pod.Name)
	}
	// Updating a pod that is already in activeQ, should not change it.
	q.Update(logger, unschedulablePodInfo.Pod, unschedulablePodInfo.Pod)
	if len(q.unschedulablePods.podInfoMap) != 0 {
		t.Error("Expected unschedulablePods to be empty.")
	}
	if _, exists, _ := q.activeQ.Get(newQueuedPodInfoForLookup(unschedulablePodInfo.Pod)); !exists {
		t.Errorf("Expected: %v to be added to activeQ.", unschedulablePodInfo.Pod.Name)
	}
	if p, err := q.Pop(); err != nil || p.Pod != highPriNominatedPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", highPriorityPodInfo.Pod.Name, p.Pod.Name)
	}

	// Updating a pod that is in backoff queue and it is still backing off
	// pod will not be moved to active queue, and it will be updated in backoff queue
	podInfo := q.newQueuedPodInfo(medPriorityPodInfo.Pod)
	if err := q.podBackoffQ.Add(podInfo); err != nil {
		t.Errorf("adding pod to backoff queue error: %v", err)
	}
	q.Update(logger, podInfo.Pod, podInfo.Pod)
	rawPodInfo, err := q.podBackoffQ.Pop()
	podGotFromBackoffQ := rawPodInfo.(*framework.QueuedPodInfo).Pod
	if err != nil || podGotFromBackoffQ != medPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", medPriorityPodInfo.Pod.Name, podGotFromBackoffQ.Name)
	}

	// updating a pod which is in unschedulable queue, and it is still backing off,
	// we will move it to backoff queue
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(medPriorityPodInfo.Pod), q.SchedulingCycle())
	if len(q.unschedulablePods.podInfoMap) != 1 {
		t.Error("Expected unschedulablePods to be 1.")
	}
	updatedPod := medPriorityPodInfo.Pod.DeepCopy()
	updatedPod.Annotations["foo"] = "test"
	q.Update(logger, medPriorityPodInfo.Pod, updatedPod)
	rawPodInfo, err = q.podBackoffQ.Pop()
	podGotFromBackoffQ = rawPodInfo.(*framework.QueuedPodInfo).Pod
	if err != nil || podGotFromBackoffQ != updatedPod {
		t.Errorf("Expected: %v after Pop, but got: %v", updatedPod.Name, podGotFromBackoffQ.Name)
	}

	// updating a pod which is in unschedulable queue, and it is not backing off,
	// we will move it to active queue
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(medPriorityPodInfo.Pod), q.SchedulingCycle())
	if len(q.unschedulablePods.podInfoMap) != 1 {
		t.Error("Expected unschedulablePods to be 1.")
	}
	updatedPod = medPriorityPodInfo.Pod.DeepCopy()
	updatedPod.Annotations["foo"] = "test1"
	// Move clock by podInitialBackoffDuration, so that pods in the unschedulablePods would pass the backing off,
	// and the pods will be moved into activeQ.
	c.Step(q.podInitialBackoffDuration)
	q.Update(logger, medPriorityPodInfo.Pod, updatedPod)
	if p, err := q.Pop(); err != nil || p.Pod != updatedPod {
		t.Errorf("Expected: %v after Pop, but got: %v", updatedPod.Name, p.Pod.Name)
	}
}

func TestPriorityQueue_Delete(t *testing.T) {
	objs := []runtime.Object{highPriorityPodInfo.Pod, unschedulablePodInfo.Pod}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	q.Update(logger, highPriorityPodInfo.Pod, highPriNominatedPodInfo.Pod)
	q.Add(logger, unschedulablePodInfo.Pod)
	if err := q.Delete(highPriNominatedPodInfo.Pod); err != nil {
		t.Errorf("delete failed: %v", err)
	}
	if _, exists, _ := q.activeQ.Get(newQueuedPodInfoForLookup(unschedulablePodInfo.Pod)); !exists {
		t.Errorf("Expected %v to be in activeQ.", unschedulablePodInfo.Pod.Name)
	}
	if _, exists, _ := q.activeQ.Get(newQueuedPodInfoForLookup(highPriNominatedPodInfo.Pod)); exists {
		t.Errorf("Didn't expect %v to be in activeQ.", highPriorityPodInfo.Pod.Name)
	}
	if len(q.nominator.nominatedPods) != 1 {
		t.Errorf("Expected nomindatePods to have only 'unschedulablePodInfo': %v", q.nominator.nominatedPods)
	}
	if err := q.Delete(unschedulablePodInfo.Pod); err != nil {
		t.Errorf("delete failed: %v", err)
	}
	if len(q.nominator.nominatedPods) != 0 {
		t.Errorf("Expected nomindatePods to be empty: %v", q.nominator)
	}
}

func TestPriorityQueue_Activate(t *testing.T) {
	tests := []struct {
		name                        string
		qPodInfoInUnschedulablePods []*framework.QueuedPodInfo
		qPodInfoInPodBackoffQ       []*framework.QueuedPodInfo
		qPodInfoInActiveQ           []*framework.QueuedPodInfo
		qPodInfoToActivate          *framework.QueuedPodInfo
		want                        []*framework.QueuedPodInfo
	}{
		{
			name:               "pod already in activeQ",
			qPodInfoInActiveQ:  []*framework.QueuedPodInfo{{PodInfo: highPriNominatedPodInfo}},
			qPodInfoToActivate: &framework.QueuedPodInfo{PodInfo: highPriNominatedPodInfo},
			want:               []*framework.QueuedPodInfo{{PodInfo: highPriNominatedPodInfo}}, // 1 already active
		},
		{
			name:               "pod not in unschedulablePods/podBackoffQ",
			qPodInfoToActivate: &framework.QueuedPodInfo{PodInfo: highPriNominatedPodInfo},
			want:               []*framework.QueuedPodInfo{},
		},
		{
			name:                        "pod in unschedulablePods",
			qPodInfoInUnschedulablePods: []*framework.QueuedPodInfo{{PodInfo: highPriNominatedPodInfo}},
			qPodInfoToActivate:          &framework.QueuedPodInfo{PodInfo: highPriNominatedPodInfo},
			want:                        []*framework.QueuedPodInfo{{PodInfo: highPriNominatedPodInfo}},
		},
		{
			name:                  "pod in backoffQ",
			qPodInfoInPodBackoffQ: []*framework.QueuedPodInfo{{PodInfo: highPriNominatedPodInfo}},
			qPodInfoToActivate:    &framework.QueuedPodInfo{PodInfo: highPriNominatedPodInfo},
			want:                  []*framework.QueuedPodInfo{{PodInfo: highPriNominatedPodInfo}},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var objs []runtime.Object
			logger, ctx := ktesting.NewTestContext(t)
			ctx, cancel := context.WithCancel(ctx)
			defer cancel()
			q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)

			// Prepare activeQ/unschedulablePods/podBackoffQ according to the table
			for _, qPodInfo := range tt.qPodInfoInActiveQ {
				q.activeQ.Add(qPodInfo)
			}

			for _, qPodInfo := range tt.qPodInfoInUnschedulablePods {
				q.unschedulablePods.addOrUpdate(qPodInfo)
			}

			for _, qPodInfo := range tt.qPodInfoInPodBackoffQ {
				q.podBackoffQ.Add(qPodInfo)
			}

			// Activate specific pod according to the table
			q.Activate(logger, map[string]*v1.Pod{"test_pod": tt.qPodInfoToActivate.PodInfo.Pod})

			// Check the result after activation by the length of activeQ
			if wantLen := len(tt.want); q.activeQ.Len() != wantLen {
				t.Errorf("length compare: want %v, got %v", wantLen, q.activeQ.Len())
			}

			// Check if the specific pod exists in activeQ
			for _, want := range tt.want {
				if _, exist, _ := q.activeQ.Get(newQueuedPodInfoForLookup(want.PodInfo.Pod)); !exist {
					t.Errorf("podInfo not exist in activeQ: want %v", want.PodInfo.Pod.Name)
				}
			}
		})
	}
}

type preEnqueuePlugin struct {
	allowlists []string
}

func (pl *preEnqueuePlugin) Name() string {
	return "preEnqueuePlugin"
}

func (pl *preEnqueuePlugin) PreEnqueue(ctx context.Context, p *v1.Pod) *framework.Status {
	for _, allowed := range pl.allowlists {
		for label := range p.Labels {
			if label == allowed {
				return nil
			}
		}
	}
	return framework.NewStatus(framework.UnschedulableAndUnresolvable, "pod name not in allowlists")
}

func TestPriorityQueue_addToActiveQ(t *testing.T) {
	tests := []struct {
		name                  string
		plugins               []framework.PreEnqueuePlugin
		pod                   *v1.Pod
		wantUnschedulablePods int
		wantSuccess           bool
	}{
		{
			name:                  "no plugins registered",
			pod:                   st.MakePod().Name("p").Label("p", "").Obj(),
			wantUnschedulablePods: 0,
			wantSuccess:           true,
		},
		{
			name:                  "preEnqueue plugin registered, pod name not in allowlists",
			plugins:               []framework.PreEnqueuePlugin{&preEnqueuePlugin{}, &preEnqueuePlugin{}},
			pod:                   st.MakePod().Name("p").Label("p", "").Obj(),
			wantUnschedulablePods: 1,
			wantSuccess:           false,
		},
		{
			name: "preEnqueue plugin registered, pod failed one preEnqueue plugin",
			plugins: []framework.PreEnqueuePlugin{
				&preEnqueuePlugin{allowlists: []string{"foo", "bar"}},
				&preEnqueuePlugin{allowlists: []string{"foo"}},
			},
			pod:                   st.MakePod().Name("bar").Label("bar", "").Obj(),
			wantUnschedulablePods: 1,
			wantSuccess:           false,
		},
		{
			name: "preEnqueue plugin registered, pod passed all preEnqueue plugins",
			plugins: []framework.PreEnqueuePlugin{
				&preEnqueuePlugin{allowlists: []string{"foo", "bar"}},
				&preEnqueuePlugin{allowlists: []string{"bar"}},
			},
			pod:                   st.MakePod().Name("bar").Label("bar", "").Obj(),
			wantUnschedulablePods: 0,
			wantSuccess:           true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			logger := klog.FromContext(ctx)
			defer cancel()

			m := map[string][]framework.PreEnqueuePlugin{"": tt.plugins}
			q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), []runtime.Object{tt.pod}, WithPreEnqueuePluginMap(m),
				WithPodInitialBackoffDuration(time.Second*30), WithPodMaxBackoffDuration(time.Second*60))
			got, _ := q.addToActiveQ(logger, q.newQueuedPodInfo(tt.pod))
			if got != tt.wantSuccess {
				t.Errorf("Unexpected result: want %v, but got %v", tt.wantSuccess, got)
			}
			if tt.wantUnschedulablePods != len(q.unschedulablePods.podInfoMap) {
				t.Errorf("Unexpected unschedulablePods: want %v, but got %v", tt.wantUnschedulablePods, len(q.unschedulablePods.podInfoMap))
			}

			// Simulate an update event.
			clone := tt.pod.DeepCopy()
			metav1.SetMetaDataAnnotation(&clone.ObjectMeta, "foo", "")
			q.Update(logger, tt.pod, clone)
			// Ensure the pod is still located in unschedulablePods.
			if tt.wantUnschedulablePods != len(q.unschedulablePods.podInfoMap) {
				t.Errorf("Unexpected unschedulablePods: want %v, but got %v", tt.wantUnschedulablePods, len(q.unschedulablePods.podInfoMap))
			}
		})
	}
}

func BenchmarkMoveAllToActiveOrBackoffQueue(b *testing.B) {
	tests := []struct {
		name      string
		moveEvent framework.ClusterEvent
	}{
		{
			name:      "baseline",
			moveEvent: UnschedulableTimeout,
		},
		{
			name:      "worst",
			moveEvent: NodeAdd,
		},
		{
			name: "random",
			// leave "moveEvent" unspecified
		},
	}

	podTemplates := []*v1.Pod{
		highPriorityPodInfo.Pod, highPriNominatedPodInfo.Pod,
		medPriorityPodInfo.Pod, unschedulablePodInfo.Pod,
	}

	events := []framework.ClusterEvent{
		NodeAdd,
		NodeTaintChange,
		NodeAllocatableChange,
		NodeConditionChange,
		NodeLabelChange,
		PvcAdd,
		PvcUpdate,
		PvAdd,
		PvUpdate,
		StorageClassAdd,
		StorageClassUpdate,
		CSINodeAdd,
		CSINodeUpdate,
		CSIDriverAdd,
		CSIDriverUpdate,
		CSIStorageCapacityAdd,
		CSIStorageCapacityUpdate,
	}

	pluginNum := 20
	var plugins []string
	// Mimic that we have 20 plugins loaded in runtime.
	for i := 0; i < pluginNum; i++ {
		plugins = append(plugins, fmt.Sprintf("fake-plugin-%v", i))
	}

	for _, tt := range tests {
		for _, podsInUnschedulablePods := range []int{1000, 5000} {
			b.Run(fmt.Sprintf("%v-%v", tt.name, podsInUnschedulablePods), func(b *testing.B) {
				logger, _ := ktesting.NewTestContext(b)
				for i := 0; i < b.N; i++ {
					b.StopTimer()
					c := testingclock.NewFakeClock(time.Now())

					m := make(map[framework.ClusterEvent]sets.Set[string])
					// - All plugins registered for events[0], which is NodeAdd.
					// - 1/2 of plugins registered for events[1]
					// - 1/3 of plugins registered for events[2]
					// - ...
					for j := 0; j < len(events); j++ {
						m[events[j]] = sets.New[string]()
						for k := 0; k < len(plugins); k++ {
							if (k+1)%(j+1) == 0 {
								m[events[j]].Insert(plugins[k])
							}
						}
					}

					ctx, cancel := context.WithCancel(context.Background())
					defer cancel()
					q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c), WithClusterEventMap(m))

					// Init pods in unschedulablePods.
					for j := 0; j < podsInUnschedulablePods; j++ {
						p := podTemplates[j%len(podTemplates)].DeepCopy()
						p.Name, p.UID = fmt.Sprintf("%v-%v", p.Name, j), types.UID(fmt.Sprintf("%v-%v", p.UID, j))
						var podInfo *framework.QueuedPodInfo
						// The ultimate goal of composing each PodInfo is to cover the path that intersects
						// (unschedulable) plugin names with the plugins that register the moveEvent,
						// here the rational is:
						// - in baseline case, don't inject unschedulable plugin names, so podMatchesEvent()
						//   never gets executed.
						// - in worst case, make both ends (of the intersection) a big number,i.e.,
						//   M intersected with N instead of M with 1 (or 1 with N)
						// - in random case, each pod failed by a random plugin, and also the moveEvent
						//   is randomized.
						if tt.name == "baseline" {
							podInfo = q.newQueuedPodInfo(p)
						} else if tt.name == "worst" {
							// Each pod failed by all plugins.
							podInfo = q.newQueuedPodInfo(p, plugins...)
						} else {
							// Random case.
							podInfo = q.newQueuedPodInfo(p, plugins[j%len(plugins)])
						}
						q.AddUnschedulableIfNotPresent(logger, podInfo, q.SchedulingCycle())
					}

					b.StartTimer()
					if tt.moveEvent.Resource != "" {
						q.MoveAllToActiveOrBackoffQueue(logger, tt.moveEvent, nil)
					} else {
						// Random case.
						q.MoveAllToActiveOrBackoffQueue(logger, events[i%len(events)], nil)
					}
				}
			})
		}
	}
}

func TestPriorityQueue_MoveAllToActiveOrBackoffQueue(t *testing.T) {
	c := testingclock.NewFakeClock(time.Now())
	m := map[framework.ClusterEvent]sets.Set[string]{
		{Resource: framework.Node, ActionType: framework.Add}: sets.New("fooPlugin"),
	}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c), WithClusterEventMap(m))
	q.Add(logger, medPriorityPodInfo.Pod)
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(unschedulablePodInfo.Pod, "fooPlugin"), q.SchedulingCycle())
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(highPriorityPodInfo.Pod, "fooPlugin"), q.SchedulingCycle())
	// Construct a Pod, but don't associate its scheduler failure to any plugin
	hpp1 := highPriorityPodInfo.Pod.DeepCopy()
	hpp1.Name = "hpp1"
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(hpp1), q.SchedulingCycle())
	// Construct another Pod, and associate its scheduler failure to plugin "barPlugin".
	hpp2 := highPriorityPodInfo.Pod.DeepCopy()
	hpp2.Name = "hpp2"
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(hpp2, "barPlugin"), q.SchedulingCycle())
	// Pods is still backing off, move the pod into backoffQ.
	q.MoveAllToActiveOrBackoffQueue(logger, NodeAdd, nil)
	if q.activeQ.Len() != 1 {
		t.Errorf("Expected 1 item to be in activeQ, but got: %v", q.activeQ.Len())
	}
	// hpp2 won't be moved.
	if q.podBackoffQ.Len() != 3 {
		t.Fatalf("Expected 3 items to be in podBackoffQ, but got: %v", q.podBackoffQ.Len())
	}

	// pop out the pods in the backoffQ.
	for q.podBackoffQ.Len() != 0 {
		q.podBackoffQ.Pop()
	}

	q.schedulingCycle++
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(unschedulablePodInfo.Pod, "fooPlugin"), q.SchedulingCycle())
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(highPriorityPodInfo.Pod, "fooPlugin"), q.SchedulingCycle())
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(hpp1), q.SchedulingCycle())
	for _, pod := range []*v1.Pod{unschedulablePodInfo.Pod, highPriorityPodInfo.Pod, hpp1, hpp2} {
		if q.unschedulablePods.get(pod) == nil {
			t.Errorf("Expected %v in the unschedulablePods", pod.Name)
		}
	}
	// Move clock by podInitialBackoffDuration, so that pods in the unschedulablePods would pass the backing off,
	// and the pods will be moved into activeQ.
	c.Step(q.podInitialBackoffDuration)
	q.MoveAllToActiveOrBackoffQueue(logger, NodeAdd, nil)
	// hpp2 won't be moved regardless of its backoff timer.
	if q.activeQ.Len() != 4 {
		t.Errorf("Expected 4 items to be in activeQ, but got: %v", q.activeQ.Len())
	}
	if q.podBackoffQ.Len() != 0 {
		t.Errorf("Expected 0 item to be in podBackoffQ, but got: %v", q.podBackoffQ.Len())
	}
}

// TestPriorityQueue_AssignedPodAdded tests AssignedPodAdded. It checks that
// when a pod with pod affinity is in unschedulablePods and another pod with a
// matching label is added, the unschedulable pod is moved to activeQ.
func TestPriorityQueue_AssignedPodAdded(t *testing.T) {
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	affinityPod := st.MakePod().Name("afp").Namespace("ns1").UID("upns1").Annotation("annot2", "val2").Priority(mediumPriority).NominatedNodeName("node1").PodAffinityExists("service", "region", st.PodAffinityWithRequiredReq).Obj()
	labelPod := st.MakePod().Name("lbp").Namespace(affinityPod.Namespace).Label("service", "securityscan").Node("node1").Obj()

	c := testingclock.NewFakeClock(time.Now())
	m := map[framework.ClusterEvent]sets.Set[string]{AssignedPodAdd: sets.New("fakePlugin")}
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c), WithClusterEventMap(m))
	q.Add(logger, medPriorityPodInfo.Pod)
	// Add a couple of pods to the unschedulablePods.
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(unschedulablePodInfo.Pod, "fakePlugin"), q.SchedulingCycle())
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(affinityPod, "fakePlugin"), q.SchedulingCycle())

	// Move clock to make the unschedulable pods complete backoff.
	c.Step(DefaultPodInitialBackoffDuration + time.Second)
	// Simulate addition of an assigned pod. The pod has matching labels for
	// affinityPod. So, affinityPod should go to activeQ.
	q.AssignedPodAdded(logger, labelPod)
	if getUnschedulablePod(q, affinityPod) != nil {
		t.Error("affinityPod is still in the unschedulablePods.")
	}
	if _, exists, _ := q.activeQ.Get(newQueuedPodInfoForLookup(affinityPod)); !exists {
		t.Error("affinityPod is not moved to activeQ.")
	}
	// Check that the other pod is still in the unschedulablePods.
	if getUnschedulablePod(q, unschedulablePodInfo.Pod) == nil {
		t.Error("unschedulablePodInfo is not in the unschedulablePods.")
	}
}

func TestPriorityQueue_NominatedPodsForNode(t *testing.T) {
	objs := []runtime.Object{medPriorityPodInfo.Pod, unschedulablePodInfo.Pod, highPriorityPodInfo.Pod}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	q.Add(logger, medPriorityPodInfo.Pod)
	q.Add(logger, unschedulablePodInfo.Pod)
	q.Add(logger, highPriorityPodInfo.Pod)
	if p, err := q.Pop(); err != nil || p.Pod != highPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", highPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
	expectedList := []*framework.PodInfo{medPriorityPodInfo, unschedulablePodInfo}
	podInfos := q.NominatedPodsForNode("node1")
	if diff := cmp.Diff(expectedList, podInfos); diff != "" {
		t.Errorf("Unexpected list of nominated Pods for node: (-want, +got):\n%s", diff)
	}
	podInfos[0].Pod.Name = "not mpp"
	if diff := cmp.Diff(podInfos, q.NominatedPodsForNode("node1")); diff == "" {
		t.Error("Expected list of nominated Pods for node2 is different from podInfos")
	}
	if len(q.NominatedPodsForNode("node2")) != 0 {
		t.Error("Expected list of nominated Pods for node2 to be empty.")
	}
}

func TestPriorityQueue_NominatedPodDeleted(t *testing.T) {
	tests := []struct {
		name      string
		podInfo   *framework.PodInfo
		deletePod bool
		want      bool
	}{
		{
			name:    "alive pod gets added into PodNominator",
			podInfo: medPriorityPodInfo,
			want:    true,
		},
		{
			name:      "deleted pod shouldn't be added into PodNominator",
			podInfo:   highPriNominatedPodInfo,
			deletePod: true,
			want:      false,
		},
		{
			name:    "pod without .status.nominatedPodName specified shouldn't be added into PodNominator",
			podInfo: highPriorityPodInfo,
			want:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			logger, _ := ktesting.NewTestContext(t)
			cs := fake.NewSimpleClientset(tt.podInfo.Pod)
			informerFactory := informers.NewSharedInformerFactory(cs, 0)
			podLister := informerFactory.Core().V1().Pods().Lister()

			// Build a PriorityQueue.
			q := NewPriorityQueue(newDefaultQueueSort(), informerFactory, WithPodLister(podLister))
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			informerFactory.Start(ctx.Done())
			informerFactory.WaitForCacheSync(ctx.Done())

			if tt.deletePod {
				// Simulate that the test pod gets deleted physically.
				informerFactory.Core().V1().Pods().Informer().GetStore().Delete(tt.podInfo.Pod)
			}

			q.AddNominatedPod(logger, tt.podInfo, nil)

			if got := len(q.NominatedPodsForNode(tt.podInfo.Pod.Status.NominatedNodeName)) == 1; got != tt.want {
				t.Errorf("Want %v, but got %v", tt.want, got)
			}
		})
	}
}

func TestPriorityQueue_PendingPods(t *testing.T) {
	makeSet := func(pods []*v1.Pod) map[*v1.Pod]struct{} {
		pendingSet := map[*v1.Pod]struct{}{}
		for _, p := range pods {
			pendingSet[p] = struct{}{}
		}
		return pendingSet
	}

	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort())
	q.Add(logger, medPriorityPodInfo.Pod)
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(unschedulablePodInfo.Pod), q.SchedulingCycle())
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(highPriorityPodInfo.Pod), q.SchedulingCycle())

	expectedSet := makeSet([]*v1.Pod{medPriorityPodInfo.Pod, unschedulablePodInfo.Pod, highPriorityPodInfo.Pod})
	gotPods, gotSummary := q.PendingPods()
	if !reflect.DeepEqual(expectedSet, makeSet(gotPods)) {
		t.Error("Unexpected list of pending Pods.")
	}
	if wantSummary := fmt.Sprintf(pendingPodsSummary, 1, 0, 2); wantSummary != gotSummary {
		t.Errorf("Unexpected pending pods summary: want %v, but got %v.", wantSummary, gotSummary)
	}
	// Move all to active queue. We should still see the same set of pods.
	q.MoveAllToActiveOrBackoffQueue(logger, TestEvent, nil)
	gotPods, gotSummary = q.PendingPods()
	if !reflect.DeepEqual(expectedSet, makeSet(gotPods)) {
		t.Error("Unexpected list of pending Pods.")
	}
	if wantSummary := fmt.Sprintf(pendingPodsSummary, 1, 2, 0); wantSummary != gotSummary {
		t.Errorf("Unexpected pending pods summary: want %v, but got %v.", wantSummary, gotSummary)
	}
}

func TestPriorityQueue_UpdateNominatedPodForNode(t *testing.T) {
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	objs := []runtime.Object{medPriorityPodInfo.Pod, unschedulablePodInfo.Pod, highPriorityPodInfo.Pod, scheduledPodInfo.Pod}
	q := NewTestQueueWithObjects(ctx, newDefaultQueueSort(), objs)
	if err := q.Add(logger, medPriorityPodInfo.Pod); err != nil {
		t.Errorf("add failed: %v", err)
	}
	// Update unschedulablePodInfo on a different node than specified in the pod.
	q.AddNominatedPod(logger, mustNewTestPodInfo(t, unschedulablePodInfo.Pod),
		&framework.NominatingInfo{NominatingMode: framework.ModeOverride, NominatedNodeName: "node5"})

	// Update nominated node name of a pod on a node that is not specified in the pod object.
	q.AddNominatedPod(logger, mustNewTestPodInfo(t, highPriorityPodInfo.Pod),
		&framework.NominatingInfo{NominatingMode: framework.ModeOverride, NominatedNodeName: "node2"})
	expectedNominatedPods := &nominator{
		nominatedPodToNode: map[types.UID]string{
			medPriorityPodInfo.Pod.UID:   "node1",
			highPriorityPodInfo.Pod.UID:  "node2",
			unschedulablePodInfo.Pod.UID: "node5",
		},
		nominatedPods: map[string][]*framework.PodInfo{
			"node1": {medPriorityPodInfo},
			"node2": {highPriorityPodInfo},
			"node5": {unschedulablePodInfo},
		},
	}
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after adding pods (-want, +got):\n%s", diff)
	}
	if p, err := q.Pop(); err != nil || p.Pod != medPriorityPodInfo.Pod {
		t.Errorf("Expected: %v after Pop, but got: %v", medPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
	// List of nominated pods shouldn't change after popping them from the queue.
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after popping pods (-want, +got):\n%s", diff)
	}
	// Update one of the nominated pods that doesn't have nominatedNodeName in the
	// pod object. It should be updated correctly.
	q.AddNominatedPod(logger, highPriorityPodInfo, &framework.NominatingInfo{NominatingMode: framework.ModeOverride, NominatedNodeName: "node4"})
	expectedNominatedPods = &nominator{
		nominatedPodToNode: map[types.UID]string{
			medPriorityPodInfo.Pod.UID:   "node1",
			highPriorityPodInfo.Pod.UID:  "node4",
			unschedulablePodInfo.Pod.UID: "node5",
		},
		nominatedPods: map[string][]*framework.PodInfo{
			"node1": {medPriorityPodInfo},
			"node4": {highPriorityPodInfo},
			"node5": {unschedulablePodInfo},
		},
	}
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after updating pods (-want, +got):\n%s", diff)
	}

	// Attempt to nominate a pod that was deleted from the informer cache.
	// Nothing should change.
	q.AddNominatedPod(logger, nonExistentPodInfo, &framework.NominatingInfo{NominatingMode: framework.ModeOverride, NominatedNodeName: "node1"})
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after nominating a deleted pod (-want, +got):\n%s", diff)
	}
	// Attempt to nominate a pod that was already scheduled in the informer cache.
	// Nothing should change.
	scheduledPodCopy := scheduledPodInfo.Pod.DeepCopy()
	scheduledPodInfo.Pod.Spec.NodeName = ""
	q.AddNominatedPod(logger, mustNewTestPodInfo(t, scheduledPodCopy), &framework.NominatingInfo{NominatingMode: framework.ModeOverride, NominatedNodeName: "node1"})
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after nominating a scheduled pod (-want, +got):\n%s", diff)
	}

	// Delete a nominated pod that doesn't have nominatedNodeName in the pod
	// object. It should be deleted.
	q.DeleteNominatedPodIfExists(highPriorityPodInfo.Pod)
	expectedNominatedPods = &nominator{
		nominatedPodToNode: map[types.UID]string{
			medPriorityPodInfo.Pod.UID:   "node1",
			unschedulablePodInfo.Pod.UID: "node5",
		},
		nominatedPods: map[string][]*framework.PodInfo{
			"node1": {medPriorityPodInfo},
			"node5": {unschedulablePodInfo},
		},
	}
	if diff := cmp.Diff(q.nominator, expectedNominatedPods, nominatorCmpOpts...); diff != "" {
		t.Errorf("Unexpected diff after deleting pods (-want, +got):\n%s", diff)
	}
}

func TestPriorityQueue_NewWithOptions(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	q := NewTestQueue(ctx,
		newDefaultQueueSort(),
		WithPodInitialBackoffDuration(2*time.Second),
		WithPodMaxBackoffDuration(20*time.Second),
	)

	if q.podInitialBackoffDuration != 2*time.Second {
		t.Errorf("Unexpected pod backoff initial duration. Expected: %v, got: %v", 2*time.Second, q.podInitialBackoffDuration)
	}

	if q.podMaxBackoffDuration != 20*time.Second {
		t.Errorf("Unexpected pod backoff max duration. Expected: %v, got: %v", 2*time.Second, q.podMaxBackoffDuration)
	}
}

func TestUnschedulablePodsMap(t *testing.T) {
	var pods = []*v1.Pod{
		st.MakePod().Name("p0").Namespace("ns1").Annotation("annot1", "val1").NominatedNodeName("node1").Obj(),
		st.MakePod().Name("p1").Namespace("ns1").Annotation("annot", "val").Obj(),
		st.MakePod().Name("p2").Namespace("ns2").Annotation("annot2", "val2").Annotation("annot3", "val3").NominatedNodeName("node3").Obj(),
		st.MakePod().Name("p3").Namespace("ns4").Annotation("annot2", "val2").Annotation("annot3", "val3").NominatedNodeName("node1").Obj(),
	}
	var updatedPods = make([]*v1.Pod, len(pods))
	updatedPods[0] = pods[0].DeepCopy()
	updatedPods[1] = pods[1].DeepCopy()
	updatedPods[3] = pods[3].DeepCopy()

	tests := []struct {
		name                   string
		podsToAdd              []*v1.Pod
		expectedMapAfterAdd    map[string]*framework.QueuedPodInfo
		podsToUpdate           []*v1.Pod
		expectedMapAfterUpdate map[string]*framework.QueuedPodInfo
		podsToDelete           []*v1.Pod
		expectedMapAfterDelete map[string]*framework.QueuedPodInfo
	}{
		{
			name:      "create, update, delete subset of pods",
			podsToAdd: []*v1.Pod{pods[0], pods[1], pods[2], pods[3]},
			expectedMapAfterAdd: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[0]): {PodInfo: mustNewTestPodInfo(t, pods[0]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[1]): {PodInfo: mustNewTestPodInfo(t, pods[1]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[2]): {PodInfo: mustNewTestPodInfo(t, pods[2]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[3]): {PodInfo: mustNewTestPodInfo(t, pods[3]), UnschedulablePlugins: sets.New[string]()},
			},
			podsToUpdate: []*v1.Pod{updatedPods[0]},
			expectedMapAfterUpdate: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[0]): {PodInfo: mustNewTestPodInfo(t, updatedPods[0]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[1]): {PodInfo: mustNewTestPodInfo(t, pods[1]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[2]): {PodInfo: mustNewTestPodInfo(t, pods[2]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[3]): {PodInfo: mustNewTestPodInfo(t, pods[3]), UnschedulablePlugins: sets.New[string]()},
			},
			podsToDelete: []*v1.Pod{pods[0], pods[1]},
			expectedMapAfterDelete: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[2]): {PodInfo: mustNewTestPodInfo(t, pods[2]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[3]): {PodInfo: mustNewTestPodInfo(t, pods[3]), UnschedulablePlugins: sets.New[string]()},
			},
		},
		{
			name:      "create, update, delete all",
			podsToAdd: []*v1.Pod{pods[0], pods[3]},
			expectedMapAfterAdd: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[0]): {PodInfo: mustNewTestPodInfo(t, pods[0]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[3]): {PodInfo: mustNewTestPodInfo(t, pods[3]), UnschedulablePlugins: sets.New[string]()},
			},
			podsToUpdate: []*v1.Pod{updatedPods[3]},
			expectedMapAfterUpdate: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[0]): {PodInfo: mustNewTestPodInfo(t, pods[0]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[3]): {PodInfo: mustNewTestPodInfo(t, updatedPods[3]), UnschedulablePlugins: sets.New[string]()},
			},
			podsToDelete:           []*v1.Pod{pods[0], pods[3]},
			expectedMapAfterDelete: map[string]*framework.QueuedPodInfo{},
		},
		{
			name:      "delete non-existing and existing pods",
			podsToAdd: []*v1.Pod{pods[1], pods[2]},
			expectedMapAfterAdd: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[1]): {PodInfo: mustNewTestPodInfo(t, pods[1]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[2]): {PodInfo: mustNewTestPodInfo(t, pods[2]), UnschedulablePlugins: sets.New[string]()},
			},
			podsToUpdate: []*v1.Pod{updatedPods[1]},
			expectedMapAfterUpdate: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[1]): {PodInfo: mustNewTestPodInfo(t, updatedPods[1]), UnschedulablePlugins: sets.New[string]()},
				util.GetPodFullName(pods[2]): {PodInfo: mustNewTestPodInfo(t, pods[2]), UnschedulablePlugins: sets.New[string]()},
			},
			podsToDelete: []*v1.Pod{pods[2], pods[3]},
			expectedMapAfterDelete: map[string]*framework.QueuedPodInfo{
				util.GetPodFullName(pods[1]): {PodInfo: mustNewTestPodInfo(t, updatedPods[1]), UnschedulablePlugins: sets.New[string]()},
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			upm := newUnschedulablePods(nil, nil)
			for _, p := range test.podsToAdd {
				upm.addOrUpdate(newQueuedPodInfoForLookup(p))
			}
			if !reflect.DeepEqual(upm.podInfoMap, test.expectedMapAfterAdd) {
				t.Errorf("Unexpected map after adding pods. Expected: %v, got: %v",
					test.expectedMapAfterAdd, upm.podInfoMap)
			}

			if len(test.podsToUpdate) > 0 {
				for _, p := range test.podsToUpdate {
					upm.addOrUpdate(newQueuedPodInfoForLookup(p))
				}
				if !reflect.DeepEqual(upm.podInfoMap, test.expectedMapAfterUpdate) {
					t.Errorf("Unexpected map after updating pods. Expected: %v, got: %v",
						test.expectedMapAfterUpdate, upm.podInfoMap)
				}
			}
			for _, p := range test.podsToDelete {
				upm.delete(p, false)
			}
			if !reflect.DeepEqual(upm.podInfoMap, test.expectedMapAfterDelete) {
				t.Errorf("Unexpected map after deleting pods. Expected: %v, got: %v",
					test.expectedMapAfterDelete, upm.podInfoMap)
			}
			upm.clear()
			if len(upm.podInfoMap) != 0 {
				t.Errorf("Expected the map to be empty, but has %v elements.", len(upm.podInfoMap))
			}
		})
	}
}

func TestSchedulingQueue_Close(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort())
	wg := sync.WaitGroup{}
	wg.Add(1)
	go func() {
		defer wg.Done()
		pod, err := q.Pop()
		if err != nil {
			t.Errorf("Expected nil err from Pop() if queue is closed, but got %q", err.Error())
		}
		if pod != nil {
			t.Errorf("Expected pod nil from Pop() if queue is closed, but got: %v", pod)
		}
	}()
	q.Close()
	wg.Wait()
}

// TestRecentlyTriedPodsGoBack tests that pods which are recently tried and are
// unschedulable go behind other pods with the same priority. This behavior
// ensures that an unschedulable pod does not block head of the queue when there
// are frequent events that move pods to the active queue.
func TestRecentlyTriedPodsGoBack(t *testing.T) {
	c := testingclock.NewFakeClock(time.Now())
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c))
	// Add a few pods to priority queue.
	for i := 0; i < 5; i++ {
		p := st.MakePod().Name(fmt.Sprintf("test-pod-%v", i)).Namespace("ns1").UID(fmt.Sprintf("tp00%v", i)).Priority(highPriority).Node("node1").NominatedNodeName("node1").Obj()
		q.Add(logger, p)
	}
	c.Step(time.Microsecond)
	// Simulate a pod being popped by the scheduler, determined unschedulable, and
	// then moved back to the active queue.
	p1, err := q.Pop()
	if err != nil {
		t.Errorf("Error while popping the head of the queue: %v", err)
	}
	// Update pod condition to unschedulable.
	podutil.UpdatePodCondition(&p1.PodInfo.Pod.Status, &v1.PodCondition{
		Type:          v1.PodScheduled,
		Status:        v1.ConditionFalse,
		Reason:        v1.PodReasonUnschedulable,
		Message:       "fake scheduling failure",
		LastProbeTime: metav1.Now(),
	})
	// Put in the unschedulable queue.
	q.AddUnschedulableIfNotPresent(logger, p1, q.SchedulingCycle())
	c.Step(DefaultPodInitialBackoffDuration)
	// Move all unschedulable pods to the active queue.
	q.MoveAllToActiveOrBackoffQueue(logger, UnschedulableTimeout, nil)
	// Simulation is over. Now let's pop all pods. The pod popped first should be
	// the last one we pop here.
	for i := 0; i < 5; i++ {
		p, err := q.Pop()
		if err != nil {
			t.Errorf("Error while popping pods from the queue: %v", err)
		}
		if (i == 4) != (p1 == p) {
			t.Errorf("A pod tried before is not the last pod popped: i: %v, pod name: %v", i, p.PodInfo.Pod.Name)
		}
	}
}

// TestPodFailedSchedulingMultipleTimesDoesNotBlockNewerPod tests
// that a pod determined as unschedulable multiple times doesn't block any newer pod.
// This behavior ensures that an unschedulable pod does not block head of the queue when there
// are frequent events that move pods to the active queue.
func TestPodFailedSchedulingMultipleTimesDoesNotBlockNewerPod(t *testing.T) {
	c := testingclock.NewFakeClock(time.Now())
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c))

	// Add an unschedulable pod to a priority queue.
	// This makes a situation that the pod was tried to schedule
	// and had been determined unschedulable so far
	unschedulablePod := st.MakePod().Name("test-pod-unscheduled").Namespace("ns1").UID("tp001").Priority(highPriority).NominatedNodeName("node1").Obj()

	// Update pod condition to unschedulable.
	podutil.UpdatePodCondition(&unschedulablePod.Status, &v1.PodCondition{
		Type:    v1.PodScheduled,
		Status:  v1.ConditionFalse,
		Reason:  v1.PodReasonUnschedulable,
		Message: "fake scheduling failure",
	})

	// Put in the unschedulable queue
	q.AddUnschedulableIfNotPresent(logger, newQueuedPodInfoForLookup(unschedulablePod), q.SchedulingCycle())
	// Move clock to make the unschedulable pods complete backoff.
	c.Step(DefaultPodInitialBackoffDuration + time.Second)
	// Move all unschedulable pods to the active queue.
	q.MoveAllToActiveOrBackoffQueue(logger, UnschedulableTimeout, nil)

	// Simulate a pod being popped by the scheduler,
	// At this time, unschedulable pod should be popped.
	p1, err := q.Pop()
	if err != nil {
		t.Errorf("Error while popping the head of the queue: %v", err)
	}
	if p1.Pod != unschedulablePod {
		t.Errorf("Expected that test-pod-unscheduled was popped, got %v", p1.Pod.Name)
	}

	// Assume newer pod was added just after unschedulable pod
	// being popped and before being pushed back to the queue.
	newerPod := st.MakePod().Name("test-newer-pod").Namespace("ns1").UID("tp002").CreationTimestamp(metav1.Now()).Priority(highPriority).NominatedNodeName("node1").Obj()
	q.Add(logger, newerPod)

	// And then unschedulablePodInfo was determined as unschedulable AGAIN.
	podutil.UpdatePodCondition(&unschedulablePod.Status, &v1.PodCondition{
		Type:    v1.PodScheduled,
		Status:  v1.ConditionFalse,
		Reason:  v1.PodReasonUnschedulable,
		Message: "fake scheduling failure",
	})

	// And then, put unschedulable pod to the unschedulable queue
	q.AddUnschedulableIfNotPresent(logger, newQueuedPodInfoForLookup(unschedulablePod), q.SchedulingCycle())
	// Move clock to make the unschedulable pods complete backoff.
	c.Step(DefaultPodInitialBackoffDuration + time.Second)
	// Move all unschedulable pods to the active queue.
	q.MoveAllToActiveOrBackoffQueue(logger, UnschedulableTimeout, nil)

	// At this time, newerPod should be popped
	// because it is the oldest tried pod.
	p2, err2 := q.Pop()
	if err2 != nil {
		t.Errorf("Error while popping the head of the queue: %v", err2)
	}
	if p2.Pod != newerPod {
		t.Errorf("Expected that test-newer-pod was popped, got %v", p2.Pod.Name)
	}
}

// TestHighPriorityBackoff tests that a high priority pod does not block
// other pods if it is unschedulable
func TestHighPriorityBackoff(t *testing.T) {
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort())

	midPod := st.MakePod().Name("test-midpod").Namespace("ns1").UID("tp-mid").Priority(midPriority).NominatedNodeName("node1").Obj()
	highPod := st.MakePod().Name("test-highpod").Namespace("ns1").UID("tp-high").Priority(highPriority).NominatedNodeName("node1").Obj()
	q.Add(logger, midPod)
	q.Add(logger, highPod)
	// Simulate a pod being popped by the scheduler, determined unschedulable, and
	// then moved back to the active queue.
	p, err := q.Pop()
	if err != nil {
		t.Errorf("Error while popping the head of the queue: %v", err)
	}
	if p.Pod != highPod {
		t.Errorf("Expected to get high priority pod, got: %v", p)
	}
	// Update pod condition to unschedulable.
	podutil.UpdatePodCondition(&p.Pod.Status, &v1.PodCondition{
		Type:    v1.PodScheduled,
		Status:  v1.ConditionFalse,
		Reason:  v1.PodReasonUnschedulable,
		Message: "fake scheduling failure",
	})
	// Put in the unschedulable queue.
	q.AddUnschedulableIfNotPresent(logger, p, q.SchedulingCycle())
	// Move all unschedulable pods to the active queue.
	q.MoveAllToActiveOrBackoffQueue(logger, TestEvent, nil)

	p, err = q.Pop()
	if err != nil {
		t.Errorf("Error while popping the head of the queue: %v", err)
	}
	if p.Pod != midPod {
		t.Errorf("Expected to get mid priority pod, got: %v", p)
	}
}

// TestHighPriorityFlushUnschedulablePodsLeftover tests that pods will be moved to
// activeQ after one minutes if it is in unschedulablePods.
func TestHighPriorityFlushUnschedulablePodsLeftover(t *testing.T) {
	c := testingclock.NewFakeClock(time.Now())
	m := map[framework.ClusterEvent]sets.Set[string]{
		NodeAdd: sets.New("fakePlugin"),
	}
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c), WithClusterEventMap(m))
	midPod := st.MakePod().Name("test-midpod").Namespace("ns1").UID("tp-mid").Priority(midPriority).NominatedNodeName("node1").Obj()
	highPod := st.MakePod().Name("test-highpod").Namespace("ns1").UID("tp-high").Priority(highPriority).NominatedNodeName("node1").Obj()

	// Update pod condition to highPod.
	podutil.UpdatePodCondition(&highPod.Status, &v1.PodCondition{
		Type:    v1.PodScheduled,
		Status:  v1.ConditionFalse,
		Reason:  v1.PodReasonUnschedulable,
		Message: "fake scheduling failure",
	})

	// Update pod condition to midPod.
	podutil.UpdatePodCondition(&midPod.Status, &v1.PodCondition{
		Type:    v1.PodScheduled,
		Status:  v1.ConditionFalse,
		Reason:  v1.PodReasonUnschedulable,
		Message: "fake scheduling failure",
	})

	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(highPod, "fakePlugin"), q.SchedulingCycle())
	q.AddUnschedulableIfNotPresent(logger, q.newQueuedPodInfo(midPod, "fakePlugin"), q.SchedulingCycle())
	c.Step(DefaultPodMaxInUnschedulablePodsDuration + time.Second)
	q.flushUnschedulablePodsLeftover(logger)

	if p, err := q.Pop(); err != nil || p.Pod != highPod {
		t.Errorf("Expected: %v after Pop, but got: %v", highPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
	if p, err := q.Pop(); err != nil || p.Pod != midPod {
		t.Errorf("Expected: %v after Pop, but got: %v", medPriorityPodInfo.Pod.Name, p.Pod.Name)
	}
}

func TestPriorityQueue_initPodMaxInUnschedulablePodsDuration(t *testing.T) {
	pod1 := st.MakePod().Name("test-pod-1").Namespace("ns1").UID("tp-1").NominatedNodeName("node1").Obj()
	pod2 := st.MakePod().Name("test-pod-2").Namespace("ns2").UID("tp-2").NominatedNodeName("node2").Obj()

	var timestamp = time.Now()
	pInfo1 := &framework.QueuedPodInfo{
		PodInfo:   mustNewTestPodInfo(t, pod1),
		Timestamp: timestamp.Add(-time.Second),
	}
	pInfo2 := &framework.QueuedPodInfo{
		PodInfo:   mustNewTestPodInfo(t, pod2),
		Timestamp: timestamp.Add(-2 * time.Second),
	}

	tests := []struct {
		name                              string
		podMaxInUnschedulablePodsDuration time.Duration
		operations                        []operation
		operands                          []*framework.QueuedPodInfo
		expected                          []*framework.QueuedPodInfo
	}{
		{
			name: "New priority queue by the default value of podMaxInUnschedulablePodsDuration",
			operations: []operation{
				addPodUnschedulablePods,
				addPodUnschedulablePods,
				flushUnschedulerQ,
			},
			operands: []*framework.QueuedPodInfo{pInfo1, pInfo2, nil},
			expected: []*framework.QueuedPodInfo{pInfo2, pInfo1},
		},
		{
			name:                              "New priority queue by user-defined value of podMaxInUnschedulablePodsDuration",
			podMaxInUnschedulablePodsDuration: 30 * time.Second,
			operations: []operation{
				addPodUnschedulablePods,
				addPodUnschedulablePods,
				flushUnschedulerQ,
			},
			operands: []*framework.QueuedPodInfo{pInfo1, pInfo2, nil},
			expected: []*framework.QueuedPodInfo{pInfo2, pInfo1},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			logger, ctx := ktesting.NewTestContext(t)
			ctx, cancel := context.WithCancel(ctx)
			defer cancel()
			var queue *PriorityQueue
			if test.podMaxInUnschedulablePodsDuration > 0 {
				queue = NewTestQueue(ctx, newDefaultQueueSort(),
					WithClock(testingclock.NewFakeClock(timestamp)),
					WithPodMaxInUnschedulablePodsDuration(test.podMaxInUnschedulablePodsDuration))
			} else {
				queue = NewTestQueue(ctx, newDefaultQueueSort(),
					WithClock(testingclock.NewFakeClock(timestamp)))
			}

			var podInfoList []*framework.QueuedPodInfo

			for i, op := range test.operations {
				op(logger, queue, test.operands[i])
			}

			expectedLen := len(test.expected)
			if queue.activeQ.Len() != expectedLen {
				t.Fatalf("Expected %v items to be in activeQ, but got: %v", expectedLen, queue.activeQ.Len())
			}

			for i := 0; i < expectedLen; i++ {
				if pInfo, err := queue.activeQ.Pop(); err != nil {
					t.Errorf("Error while popping the head of the queue: %v", err)
				} else {
					podInfoList = append(podInfoList, pInfo.(*framework.QueuedPodInfo))
				}
			}

			if diff := cmp.Diff(test.expected, podInfoList); diff != "" {
				t.Errorf("Unexpected QueuedPodInfo list (-want, +got):\n%s", diff)
			}
		})
	}
}

type operation func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo)

var (
	add = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.Add(logger, pInfo.Pod)
	}
	addUnschedulablePodBackToUnschedulablePods = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.AddUnschedulableIfNotPresent(logger, pInfo, 0)
	}
	addUnschedulablePodBackToBackoffQ = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.AddUnschedulableIfNotPresent(logger, pInfo, -1)
	}
	addPodActiveQ = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.activeQ.Add(pInfo)
	}
	updatePodActiveQ = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.activeQ.Update(pInfo)
	}
	addPodUnschedulablePods = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		if !pInfo.Gated {
			// Update pod condition to unschedulable.
			podutil.UpdatePodCondition(&pInfo.Pod.Status, &v1.PodCondition{
				Type:    v1.PodScheduled,
				Status:  v1.ConditionFalse,
				Reason:  v1.PodReasonUnschedulable,
				Message: "fake scheduling failure",
			})
		}
		queue.unschedulablePods.addOrUpdate(pInfo)
	}
	deletePod = func(_ klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.Delete(pInfo.Pod)
	}
	updatePodQueueable = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		newPod := pInfo.Pod.DeepCopy()
		newPod.Labels = map[string]string{"queueable": ""}
		queue.Update(logger, pInfo.Pod, newPod)
	}
	addPodBackoffQ = func(logger klog.Logger, queue *PriorityQueue, pInfo *framework.QueuedPodInfo) {
		queue.podBackoffQ.Add(pInfo)
	}
	moveAllToActiveOrBackoffQ = func(logger klog.Logger, queue *PriorityQueue, _ *framework.QueuedPodInfo) {
		queue.MoveAllToActiveOrBackoffQueue(logger, UnschedulableTimeout, nil)
	}
	flushBackoffQ = func(logger klog.Logger, queue *PriorityQueue, _ *framework.QueuedPodInfo) {
		queue.clock.(*testingclock.FakeClock).Step(2 * time.Second)
		queue.flushBackoffQCompleted(logger)
	}
	moveClockForward = func(logger klog.Logger, queue *PriorityQueue, _ *framework.QueuedPodInfo) {
		queue.clock.(*testingclock.FakeClock).Step(2 * time.Second)
	}
	flushUnschedulerQ = func(logger klog.Logger, queue *PriorityQueue, _ *framework.QueuedPodInfo) {
		queue.clock.(*testingclock.FakeClock).Step(queue.podMaxInUnschedulablePodsDuration)
		queue.flushUnschedulablePodsLeftover(logger)
	}
)

// TestPodTimestamp tests the operations related to QueuedPodInfo.
func TestPodTimestamp(t *testing.T) {
	pod1 := st.MakePod().Name("test-pod-1").Namespace("ns1").UID("tp-1").NominatedNodeName("node1").Obj()
	pod2 := st.MakePod().Name("test-pod-2").Namespace("ns2").UID("tp-2").NominatedNodeName("node2").Obj()

	var timestamp = time.Now()
	pInfo1 := &framework.QueuedPodInfo{
		PodInfo:   mustNewTestPodInfo(t, pod1),
		Timestamp: timestamp,
	}
	pInfo2 := &framework.QueuedPodInfo{
		PodInfo:   mustNewTestPodInfo(t, pod2),
		Timestamp: timestamp.Add(time.Second),
	}

	tests := []struct {
		name       string
		operations []operation
		operands   []*framework.QueuedPodInfo
		expected   []*framework.QueuedPodInfo
	}{
		{
			name: "add two pod to activeQ and sort them by the timestamp",
			operations: []operation{
				addPodActiveQ,
				addPodActiveQ,
			},
			operands: []*framework.QueuedPodInfo{pInfo2, pInfo1},
			expected: []*framework.QueuedPodInfo{pInfo1, pInfo2},
		},
		{
			name: "update two pod to activeQ and sort them by the timestamp",
			operations: []operation{
				updatePodActiveQ,
				updatePodActiveQ,
			},
			operands: []*framework.QueuedPodInfo{pInfo2, pInfo1},
			expected: []*framework.QueuedPodInfo{pInfo1, pInfo2},
		},
		{
			name: "add two pod to unschedulablePods then move them to activeQ and sort them by the timestamp",
			operations: []operation{
				addPodUnschedulablePods,
				addPodUnschedulablePods,
				moveClockForward,
				moveAllToActiveOrBackoffQ,
			},
			operands: []*framework.QueuedPodInfo{pInfo2, pInfo1, nil, nil},
			expected: []*framework.QueuedPodInfo{pInfo1, pInfo2},
		},
		{
			name: "add one pod to BackoffQ and move it to activeQ",
			operations: []operation{
				addPodActiveQ,
				addPodBackoffQ,
				flushBackoffQ,
				moveAllToActiveOrBackoffQ,
			},
			operands: []*framework.QueuedPodInfo{pInfo2, pInfo1, nil, nil},
			expected: []*framework.QueuedPodInfo{pInfo1, pInfo2},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			logger, ctx := ktesting.NewTestContext(t)
			ctx, cancel := context.WithCancel(ctx)
			defer cancel()
			queue := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(testingclock.NewFakeClock(timestamp)))
			var podInfoList []*framework.QueuedPodInfo

			for i, op := range test.operations {
				op(logger, queue, test.operands[i])
			}

			expectedLen := len(test.expected)
			if queue.activeQ.Len() != expectedLen {
				t.Fatalf("Expected %v items to be in activeQ, but got: %v", expectedLen, queue.activeQ.Len())
			}

			for i := 0; i < expectedLen; i++ {
				if pInfo, err := queue.activeQ.Pop(); err != nil {
					t.Errorf("Error while popping the head of the queue: %v", err)
				} else {
					podInfoList = append(podInfoList, pInfo.(*framework.QueuedPodInfo))
				}
			}

			if !reflect.DeepEqual(test.expected, podInfoList) {
				t.Errorf("Unexpected QueuedPodInfo list. Expected: %v, got: %v",
					test.expected, podInfoList)
			}
		})
	}
}

// TestPendingPodsMetric tests Prometheus metrics related with pending pods
func TestPendingPodsMetric(t *testing.T) {
	timestamp := time.Now()
	metrics.Register()
	total := 60
	queueableNum := 50
	queueable, failme := "queueable", "failme"
	// First 50 Pods are queueable.
	pInfos := makeQueuedPodInfos(queueableNum, "x", queueable, timestamp)
	// The last 10 Pods are not queueable.
	gated := makeQueuedPodInfos(total-queueableNum, "y", failme, timestamp)
	// Manually mark them as gated=true.
	for _, pInfo := range gated {
		pInfo.Gated = true
	}
	pInfos = append(pInfos, gated...)
	totalWithDelay := 20
	pInfosWithDelay := makeQueuedPodInfos(totalWithDelay, "z", queueable, timestamp.Add(2*time.Second))

	tests := []struct {
		name                       string
		operations                 []operation
		operands                   [][]*framework.QueuedPodInfo
		metricsName                string
		pluginMetricsSamplePercent int
		wants                      string
	}{
		{
			name: "add pods to activeQ and unschedulablePods",
			operations: []operation{
				addPodActiveQ,
				addPodUnschedulablePods,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:30],
				pInfos[30:],
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 30
scheduler_pending_pods{queue="backoff"} 0
scheduler_pending_pods{queue="gated"} 10
scheduler_pending_pods{queue="unschedulable"} 20
`,
		},
		{
			name: "add pods to all kinds of queues",
			operations: []operation{
				addPodActiveQ,
				addPodBackoffQ,
				addPodUnschedulablePods,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:15],
				pInfos[15:40],
				pInfos[40:],
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 15
scheduler_pending_pods{queue="backoff"} 25
scheduler_pending_pods{queue="gated"} 10
scheduler_pending_pods{queue="unschedulable"} 10
`,
		},
		{
			name: "add pods to unschedulablePods and then move all to activeQ",
			operations: []operation{
				addPodUnschedulablePods,
				moveClockForward,
				moveAllToActiveOrBackoffQ,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:total],
				{nil},
				{nil},
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 50
scheduler_pending_pods{queue="backoff"} 0
scheduler_pending_pods{queue="gated"} 10
scheduler_pending_pods{queue="unschedulable"} 0
`,
		},
		{
			name: "make some pods subject to backoff, add pods to unschedulablePods, and then move all to activeQ",
			operations: []operation{
				addPodUnschedulablePods,
				moveClockForward,
				addPodUnschedulablePods,
				moveAllToActiveOrBackoffQ,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[20:total],
				{nil},
				pInfosWithDelay[:20],
				{nil},
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 30
scheduler_pending_pods{queue="backoff"} 20
scheduler_pending_pods{queue="gated"} 10
scheduler_pending_pods{queue="unschedulable"} 0
`,
		},
		{
			name: "make some pods subject to backoff, add pods to unschedulablePods/activeQ, move all to activeQ, and finally flush backoffQ",
			operations: []operation{
				addPodUnschedulablePods,
				addPodActiveQ,
				moveAllToActiveOrBackoffQ,
				flushBackoffQ,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:40],
				pInfos[40:50],
				{nil},
				{nil},
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 50
scheduler_pending_pods{queue="backoff"} 0
scheduler_pending_pods{queue="gated"} 0
scheduler_pending_pods{queue="unschedulable"} 0
`,
		},
		{
			name: "add pods to activeQ/unschedulablePods and then delete some Pods",
			operations: []operation{
				addPodActiveQ,
				addPodUnschedulablePods,
				deletePod,
				deletePod,
				deletePod,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:30],
				pInfos[30:],
				pInfos[:2],
				pInfos[30:33],
				pInfos[50:54],
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 28
scheduler_pending_pods{queue="backoff"} 0
scheduler_pending_pods{queue="gated"} 6
scheduler_pending_pods{queue="unschedulable"} 17
`,
		},
		{
			name: "add pods to activeQ/unschedulablePods and then update some Pods as queueable",
			operations: []operation{
				addPodActiveQ,
				addPodUnschedulablePods,
				updatePodQueueable,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:30],
				pInfos[30:],
				pInfos[50:55],
			},
			metricsName: "scheduler_pending_pods",
			wants: `
# HELP scheduler_pending_pods [STABLE] Number of pending pods, by the queue type. 'active' means number of pods in activeQ; 'backoff' means number of pods in backoffQ; 'unschedulable' means number of pods in unschedulablePods that the scheduler attempted to schedule and failed; 'gated' is the number of unschedulable pods that the scheduler never attempted to schedule because they are gated.
# TYPE scheduler_pending_pods gauge
scheduler_pending_pods{queue="active"} 35
scheduler_pending_pods{queue="backoff"} 0
scheduler_pending_pods{queue="gated"} 5
scheduler_pending_pods{queue="unschedulable"} 20
`,
		},
		{
			name: "the metrics should not be recorded (pluginMetricsSamplePercent=0)",
			operations: []operation{
				add,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:1],
			},
			metricsName:                "scheduler_plugin_execution_duration_seconds",
			pluginMetricsSamplePercent: 0,
			wants: `
# HELP scheduler_plugin_execution_duration_seconds [ALPHA] Duration for running a plugin at a specific extension point.
# TYPE scheduler_plugin_execution_duration_seconds histogram
`, // the observed value will always be 0, because we don't proceed the fake clock.
		},
		{
			name: "the metrics should be recorded (pluginMetricsSamplePercent=100)",
			operations: []operation{
				add,
			},
			operands: [][]*framework.QueuedPodInfo{
				pInfos[:1],
			},
			metricsName:                "scheduler_plugin_execution_duration_seconds",
			pluginMetricsSamplePercent: 100,
			wants: `
# HELP scheduler_plugin_execution_duration_seconds [ALPHA] Duration for running a plugin at a specific extension point.
# TYPE scheduler_plugin_execution_duration_seconds histogram
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="1e-05"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="1.5000000000000002e-05"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="2.2500000000000005e-05"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="3.375000000000001e-05"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="5.062500000000001e-05"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="7.593750000000002e-05"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.00011390625000000003"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.00017085937500000006"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.0002562890625000001"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.00038443359375000017"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.0005766503906250003"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.0008649755859375004"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.0012974633789062506"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.0019461950683593758"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.0029192926025390638"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.004378938903808595"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.006568408355712893"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.009852612533569338"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.014778918800354007"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="0.02216837820053101"} 1
scheduler_plugin_execution_duration_seconds_bucket{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success",le="+Inf"} 1
scheduler_plugin_execution_duration_seconds_sum{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success"} 0
scheduler_plugin_execution_duration_seconds_count{extension_point="PreEnqueue",plugin="preEnqueuePlugin",status="Success"} 1
`, // the observed value will always be 0, because we don't proceed the fake clock.
		},
	}

	resetMetrics := func() {
		metrics.ActivePods().Set(0)
		metrics.BackoffPods().Set(0)
		metrics.UnschedulablePods().Set(0)
		metrics.GatedPods().Set(0)
		metrics.PluginExecutionDuration.Reset()
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			resetMetrics()
			logger, ctx := ktesting.NewTestContext(t)
			ctx, cancel := context.WithCancel(ctx)
			defer cancel()

			m := map[string][]framework.PreEnqueuePlugin{"": {&preEnqueuePlugin{allowlists: []string{queueable}}}}
			recorder := metrics.NewMetricsAsyncRecorder(3, 20*time.Microsecond, ctx.Done())
			queue := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(testingclock.NewFakeClock(timestamp)), WithPreEnqueuePluginMap(m), WithPluginMetricsSamplePercent(test.pluginMetricsSamplePercent), WithMetricsRecorder(*recorder))
			for i, op := range test.operations {
				for _, pInfo := range test.operands[i] {
					op(logger, queue, pInfo)
				}
			}

			recorder.FlushMetrics()

			if err := testutil.GatherAndCompare(metrics.GetGather(), strings.NewReader(test.wants), test.metricsName); err != nil {
				t.Fatal(err)
			}
		})
	}
}

// TestPerPodSchedulingMetrics makes sure pod schedule attempts is updated correctly while
// initialAttemptTimestamp stays the same during multiple add/pop operations.
func TestPerPodSchedulingMetrics(t *testing.T) {
	pod := st.MakePod().Name("test-pod").Namespace("test-ns").UID("test-uid").Obj()
	timestamp := time.Now()

	// Case 1: A pod is created and scheduled after 1 attempt. The queue operations are
	// Add -> Pop.
	c := testingclock.NewFakeClock(timestamp)
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	queue := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c))
	queue.Add(logger, pod)
	pInfo, err := queue.Pop()
	if err != nil {
		t.Fatalf("Failed to pop a pod %v", err)
	}
	checkPerPodSchedulingMetrics("Attempt once", t, pInfo, 1, timestamp)

	// Case 2: A pod is created and scheduled after 2 attempts. The queue operations are
	// Add -> Pop -> AddUnschedulableIfNotPresent -> flushUnschedulablePodsLeftover -> Pop.
	c = testingclock.NewFakeClock(timestamp)
	queue = NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c))
	queue.Add(logger, pod)
	pInfo, err = queue.Pop()
	if err != nil {
		t.Fatalf("Failed to pop a pod %v", err)
	}
	queue.AddUnschedulableIfNotPresent(logger, pInfo, 1)
	// Override clock to exceed the DefaultPodMaxInUnschedulablePodsDuration so that unschedulable pods
	// will be moved to activeQ
	c.SetTime(timestamp.Add(DefaultPodMaxInUnschedulablePodsDuration + 1))
	queue.flushUnschedulablePodsLeftover(logger)
	pInfo, err = queue.Pop()
	if err != nil {
		t.Fatalf("Failed to pop a pod %v", err)
	}
	checkPerPodSchedulingMetrics("Attempt twice", t, pInfo, 2, timestamp)

	// Case 3: Similar to case 2, but before the second pop, call update, the queue operations are
	// Add -> Pop -> AddUnschedulableIfNotPresent -> flushUnschedulablePodsLeftover -> Update -> Pop.
	c = testingclock.NewFakeClock(timestamp)
	queue = NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c))
	queue.Add(logger, pod)
	pInfo, err = queue.Pop()
	if err != nil {
		t.Fatalf("Failed to pop a pod %v", err)
	}
	queue.AddUnschedulableIfNotPresent(logger, pInfo, 1)
	// Override clock to exceed the DefaultPodMaxInUnschedulablePodsDuration so that unschedulable pods
	// will be moved to activeQ
	c.SetTime(timestamp.Add(DefaultPodMaxInUnschedulablePodsDuration + 1))
	queue.flushUnschedulablePodsLeftover(logger)
	newPod := pod.DeepCopy()
	newPod.Generation = 1
	queue.Update(logger, pod, newPod)
	pInfo, err = queue.Pop()
	if err != nil {
		t.Fatalf("Failed to pop a pod %v", err)
	}
	checkPerPodSchedulingMetrics("Attempt twice with update", t, pInfo, 2, timestamp)

	// Case 4: A gated pod is created and scheduled after lifting gate. The queue operations are
	//  Add gated pod -> check unschedulablePods -> lift gate & update pod -> Pop.
	c = testingclock.NewFakeClock(timestamp)
	// Create a queue with PreEnqueuePlugin
	m := map[string][]framework.PreEnqueuePlugin{"": {&preEnqueuePlugin{allowlists: []string{"foo"}}}}
	queue = NewTestQueue(ctx, newDefaultQueueSort(), WithClock(c), WithPreEnqueuePluginMap(m), WithPluginMetricsSamplePercent(0))

	// Create a pod without PreEnqueuePlugin label.
	gatedPod := st.MakePod().Name("gated-test-pod").Namespace("test-ns").UID("test-uid").Obj()
	err = queue.Add(logger, gatedPod)
	if err != nil {
		t.Fatalf("Failed to add a pod %v", err)
	}
	// Check pod is added to the unschedulablePods queue.
	if getUnschedulablePod(queue, gatedPod) != gatedPod {
		t.Errorf("Pod %v was not found in the unschedulablePods.", gatedPod.Name)
	}
	// Override clock to get different InitialAttemptTimestamp
	c.Step(1 * time.Minute)

	// Update pod with the required label to get it out of unschedulablePods queue.
	updateGatedPod := gatedPod.DeepCopy()
	updateGatedPod.Labels = map[string]string{"foo": ""}
	queue.Update(logger, gatedPod, updateGatedPod)

	pInfo, err = queue.Pop()
	if err != nil {
		t.Fatalf("Failed to pop a pod %v", err)
	}

	checkPerPodSchedulingMetrics("Attempt once/gated", t, pInfo, 1, timestamp.Add(1*time.Minute))
}

func TestIncomingPodsMetrics(t *testing.T) {
	timestamp := time.Now()
	metrics.Register()
	var pInfos = make([]*framework.QueuedPodInfo, 0, 3)
	for i := 1; i <= 3; i++ {
		p := &framework.QueuedPodInfo{
			PodInfo: mustNewTestPodInfo(t,
				st.MakePod().Name(fmt.Sprintf("test-pod-%d", i)).Namespace(fmt.Sprintf("ns%d", i)).UID(fmt.Sprintf("tp-%d", i)).Obj()),
			Timestamp: timestamp,
		}
		pInfos = append(pInfos, p)
	}
	tests := []struct {
		name       string
		operations []operation
		want       string
	}{
		{
			name: "add pods to activeQ",
			operations: []operation{
				add,
			},
			want: `
            scheduler_queue_incoming_pods_total{event="PodAdd",queue="active"} 3
`,
		},
		{
			name: "add pods to unschedulablePods",
			operations: []operation{
				addUnschedulablePodBackToUnschedulablePods,
			},
			want: `
             scheduler_queue_incoming_pods_total{event="ScheduleAttemptFailure",queue="unschedulable"} 3
`,
		},
		{
			name: "add pods to unschedulablePods and then move all to backoffQ",
			operations: []operation{
				addUnschedulablePodBackToUnschedulablePods,
				moveAllToActiveOrBackoffQ,
			},
			want: ` scheduler_queue_incoming_pods_total{event="ScheduleAttemptFailure",queue="unschedulable"} 3
            scheduler_queue_incoming_pods_total{event="UnschedulableTimeout",queue="backoff"} 3
`,
		},
		{
			name: "add pods to unschedulablePods and then move all to activeQ",
			operations: []operation{
				addUnschedulablePodBackToUnschedulablePods,
				moveClockForward,
				moveAllToActiveOrBackoffQ,
			},
			want: ` scheduler_queue_incoming_pods_total{event="ScheduleAttemptFailure",queue="unschedulable"} 3
            scheduler_queue_incoming_pods_total{event="UnschedulableTimeout",queue="active"} 3
`,
		},
		{
			name: "make some pods subject to backoff and add them to backoffQ, then flush backoffQ",
			operations: []operation{
				addUnschedulablePodBackToBackoffQ,
				moveClockForward,
				flushBackoffQ,
			},
			want: ` scheduler_queue_incoming_pods_total{event="BackoffComplete",queue="active"} 3
            scheduler_queue_incoming_pods_total{event="ScheduleAttemptFailure",queue="backoff"} 3
`,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			metrics.SchedulerQueueIncomingPods.Reset()
			logger, ctx := ktesting.NewTestContext(t)
			ctx, cancel := context.WithCancel(ctx)
			defer cancel()
			queue := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(testingclock.NewFakeClock(timestamp)))
			for _, op := range test.operations {
				for _, pInfo := range pInfos {
					op(logger, queue, pInfo)
				}
			}
			metricName := metrics.SchedulerSubsystem + "_" + metrics.SchedulerQueueIncomingPods.Name
			if err := testutil.CollectAndCompare(metrics.SchedulerQueueIncomingPods, strings.NewReader(queueMetricMetadata+test.want), metricName); err != nil {
				t.Errorf("unexpected collecting result:\n%s", err)
			}

		})
	}
}

func checkPerPodSchedulingMetrics(name string, t *testing.T, pInfo *framework.QueuedPodInfo, wantAttempts int, wantInitialAttemptTs time.Time) {
	if pInfo.Attempts != wantAttempts {
		t.Errorf("[%s] Pod schedule attempt unexpected, got %v, want %v", name, pInfo.Attempts, wantAttempts)
	}
	if *pInfo.InitialAttemptTimestamp != wantInitialAttemptTs {
		t.Errorf("[%s] Pod initial schedule attempt timestamp unexpected, got %v, want %v", name, *pInfo.InitialAttemptTimestamp, wantInitialAttemptTs)
	}
}

func TestBackOffFlow(t *testing.T) {
	cl := testingclock.NewFakeClock(time.Now())
	logger, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	q := NewTestQueue(ctx, newDefaultQueueSort(), WithClock(cl))
	steps := []struct {
		wantBackoff time.Duration
	}{
		{wantBackoff: time.Second},
		{wantBackoff: 2 * time.Second},
		{wantBackoff: 4 * time.Second},
		{wantBackoff: 8 * time.Second},
		{wantBackoff: 10 * time.Second},
		{wantBackoff: 10 * time.Second},
		{wantBackoff: 10 * time.Second},
	}
	pod := st.MakePod().Name("test-pod").Namespace("test-ns").UID("test-uid").Obj()

	podID := types.NamespacedName{
		Namespace: pod.Namespace,
		Name:      pod.Name,
	}
	if err := q.Add(logger, pod); err != nil {
		t.Fatal(err)
	}

	for i, step := range steps {
		t.Run(fmt.Sprintf("step %d", i), func(t *testing.T) {
			timestamp := cl.Now()
			// Simulate schedule attempt.
			podInfo, err := q.Pop()
			if err != nil {
				t.Fatal(err)
			}
			if podInfo.Attempts != i+1 {
				t.Errorf("got attempts %d, want %d", podInfo.Attempts, i+1)
			}
			if err := q.AddUnschedulableIfNotPresent(logger, podInfo, int64(i)); err != nil {
				t.Fatal(err)
			}

			// An event happens.
			q.MoveAllToActiveOrBackoffQueue(logger, UnschedulableTimeout, nil)

			if _, ok, _ := q.podBackoffQ.Get(podInfo); !ok {
				t.Errorf("pod %v is not in the backoff queue", podID)
			}

			// Check backoff duration.
			deadline := q.getBackoffTime(podInfo)
			backoff := deadline.Sub(timestamp)
			if backoff != step.wantBackoff {
				t.Errorf("got backoff %s, want %s", backoff, step.wantBackoff)
			}

			// Simulate routine that continuously flushes the backoff queue.
			cl.Step(time.Millisecond)
			q.flushBackoffQCompleted(logger)
			// Still in backoff queue after an early flush.
			if _, ok, _ := q.podBackoffQ.Get(podInfo); !ok {
				t.Errorf("pod %v is not in the backoff queue", podID)
			}
			// Moved out of the backoff queue after timeout.
			cl.Step(backoff)
			q.flushBackoffQCompleted(logger)
			if _, ok, _ := q.podBackoffQ.Get(podInfo); ok {
				t.Errorf("pod %v is still in the backoff queue", podID)
			}
		})
	}
}

func TestPodMatchesEvent(t *testing.T) {
	tests := []struct {
		name            string
		podInfo         *framework.QueuedPodInfo
		event           framework.ClusterEvent
		clusterEventMap map[framework.ClusterEvent]sets.Set[string]
		want            bool
	}{
		{
			name:    "event not registered",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj()),
			event:   EmptyEvent,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				NodeAllEvent: sets.New("foo"),
			},
			want: false,
		},
		{
			name:    "pod's failed plugin matches but event does not match",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "bar"),
			event:   AssignedPodAdd,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				NodeAllEvent: sets.New("foo", "bar"),
			},
			want: false,
		},
		{
			name:    "wildcard event wins regardless of event matching",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "bar"),
			event:   WildCardEvent,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				NodeAllEvent: sets.New("foo"),
			},
			want: true,
		},
		{
			name:    "pod's failed plugin and event both match",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "bar"),
			event:   NodeTaintChange,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				NodeAllEvent: sets.New("foo", "bar"),
			},
			want: true,
		},
		{
			name:    "pod's failed plugin registers fine-grained event",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "bar"),
			event:   NodeTaintChange,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				NodeAllEvent:    sets.New("foo"),
				NodeTaintChange: sets.New("bar"),
			},
			want: true,
		},
		{
			name:    "if pod failed by multiple plugins, a single match gets a final match",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "foo", "bar"),
			event:   NodeAdd,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				NodeAllEvent: sets.New("bar"),
			},
			want: true,
		},
		{
			name:    "plugin returns WildCardEvent and plugin name matches",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "foo"),
			event:   PvAdd,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				WildCardEvent: sets.New("foo"),
			},
			want: true,
		},
		{
			name:    "plugin returns WildCardEvent but plugin name not match",
			podInfo: newQueuedPodInfoForLookup(st.MakePod().Name("p").Obj(), "foo"),
			event:   PvAdd,
			clusterEventMap: map[framework.ClusterEvent]sets.Set[string]{
				WildCardEvent: sets.New("bar"),
			},
			want: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			q := NewTestQueue(ctx, newDefaultQueueSort())
			q.clusterEventMap = tt.clusterEventMap
			if got := q.podMatchesEvent(tt.podInfo, tt.event); got != tt.want {
				t.Errorf("Want %v, but got %v", tt.want, got)
			}
		})
	}
}

func TestMoveAllToActiveOrBackoffQueue_PreEnqueueChecks(t *testing.T) {
	var podInfos []*framework.QueuedPodInfo
	for i := 0; i < 5; i++ {
		pInfo := newQueuedPodInfoForLookup(
			st.MakePod().Name(fmt.Sprintf("p%d", i)).Priority(int32(i)).Obj(),
		)
		podInfos = append(podInfos, pInfo)
	}

	tests := []struct {
		name            string
		preEnqueueCheck PreEnqueueCheck
		podInfos        []*framework.QueuedPodInfo
		want            []string
	}{
		{
			name:     "nil PreEnqueueCheck",
			podInfos: podInfos,
			want:     []string{"p0", "p1", "p2", "p3", "p4"},
		},
		{
			name:            "move Pods with priority greater than 2",
			podInfos:        podInfos,
			preEnqueueCheck: func(pod *v1.Pod) bool { return *pod.Spec.Priority >= 2 },
			want:            []string{"p2", "p3", "p4"},
		},
		{
			name:     "move Pods with even priority and greater than 2",
			podInfos: podInfos,
			preEnqueueCheck: func(pod *v1.Pod) bool {
				return *pod.Spec.Priority%2 == 0 && *pod.Spec.Priority >= 2
			},
			want: []string{"p2", "p4"},
		},
		{
			name:     "move Pods with even and negative priority",
			podInfos: podInfos,
			preEnqueueCheck: func(pod *v1.Pod) bool {
				return *pod.Spec.Priority%2 == 0 && *pod.Spec.Priority < 0
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			logger, ctx := ktesting.NewTestContext(t)
			ctx, cancel := context.WithCancel(ctx)
			defer cancel()
			q := NewTestQueue(ctx, newDefaultQueueSort())
			for i, podInfo := range tt.podInfos {
				q.AddUnschedulableIfNotPresent(logger, podInfo, q.schedulingCycle)
				// NOTE: On Windows, time.Now() is not as precise, 2 consecutive calls may return the same timestamp,
				// resulting in 0 time delta / latency. This will cause the pods to be backed off in a random
				// order, which would cause this test to fail, since the expectation is for them to be backed off
				// in a certain order.
				// See: https://github.com/golang/go/issues/8687
				podInfo.Timestamp = podInfo.Timestamp.Add(time.Duration((i - len(tt.podInfos))) * time.Millisecond)
			}
			q.MoveAllToActiveOrBackoffQueue(logger, TestEvent, tt.preEnqueueCheck)
			var got []string
			for q.podBackoffQ.Len() != 0 {
				obj, err := q.podBackoffQ.Pop()
				if err != nil {
					t.Fatalf("Fail to pop pod from backoffQ: %v", err)
				}
				queuedPodInfo, ok := obj.(*framework.QueuedPodInfo)
				if !ok {
					t.Fatalf("Fail to convert popped obj (type %T) to *framework.QueuedPodInfo", obj)
				}
				got = append(got, queuedPodInfo.Pod.Name)
			}
			if diff := cmp.Diff(tt.want, got); diff != "" {
				t.Errorf("Unexpected diff (-want, +got):\n%s", diff)
			}
		})
	}
}

func makeQueuedPodInfos(num int, namePrefix, label string, timestamp time.Time) []*framework.QueuedPodInfo {
	var pInfos = make([]*framework.QueuedPodInfo, 0, num)
	for i := 1; i <= num; i++ {
		p := &framework.QueuedPodInfo{
			PodInfo: mustNewPodInfo(
				st.MakePod().Name(fmt.Sprintf("%v-%d", namePrefix, i)).Namespace(fmt.Sprintf("ns%d", i)).Label(label, "").UID(fmt.Sprintf("tp-%d", i)).Obj()),
			Timestamp:            timestamp,
			UnschedulablePlugins: sets.New[string](),
		}
		pInfos = append(pInfos, p)
	}
	return pInfos
}

func TestPriorityQueue_calculateBackoffDuration(t *testing.T) {
	tests := []struct {
		name                   string
		initialBackoffDuration time.Duration
		maxBackoffDuration     time.Duration
		podInfo                *framework.QueuedPodInfo
		want                   time.Duration
	}{
		{
			name:                   "normal",
			initialBackoffDuration: 1 * time.Nanosecond,
			maxBackoffDuration:     32 * time.Nanosecond,
			podInfo:                &framework.QueuedPodInfo{Attempts: 16},
			want:                   32 * time.Nanosecond,
		},
		{
			name:                   "overflow_32bit",
			initialBackoffDuration: 1 * time.Nanosecond,
			maxBackoffDuration:     math.MaxInt32 * time.Nanosecond,
			podInfo:                &framework.QueuedPodInfo{Attempts: 32},
			want:                   math.MaxInt32 * time.Nanosecond,
		},
		{
			name:                   "overflow_64bit",
			initialBackoffDuration: 1 * time.Nanosecond,
			maxBackoffDuration:     math.MaxInt64 * time.Nanosecond,
			podInfo:                &framework.QueuedPodInfo{Attempts: 64},
			want:                   math.MaxInt64 * time.Nanosecond,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			q := NewTestQueue(ctx, newDefaultQueueSort(), WithPodInitialBackoffDuration(tt.initialBackoffDuration), WithPodMaxBackoffDuration(tt.maxBackoffDuration))
			if got := q.calculateBackoffDuration(tt.podInfo); got != tt.want {
				t.Errorf("PriorityQueue.calculateBackoffDuration() = %v, want %v", got, tt.want)
			}
		})
	}
}

func mustNewTestPodInfo(t *testing.T, pod *v1.Pod) *framework.PodInfo {
	podInfo, err := framework.NewPodInfo(pod)
	if err != nil {
		t.Fatal(err)
	}
	return podInfo
}

func mustNewPodInfo(pod *v1.Pod) *framework.PodInfo {
	podInfo, err := framework.NewPodInfo(pod)
	if err != nil {
		panic(err)
	}
	return podInfo
}
