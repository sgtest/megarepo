/*
Copyright 2022 The Kubernetes Authors.

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

package dynamicresources

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"sync"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	v1 "k8s.io/api/core/v1"
	resourcev1alpha2 "k8s.io/api/resource/v1alpha2"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	apiruntime "k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/client-go/informers"
	"k8s.io/client-go/kubernetes/fake"
	cgotesting "k8s.io/client-go/testing"
	"k8s.io/klog/v2/ktesting"
	_ "k8s.io/klog/v2/ktesting/init"
	"k8s.io/kubernetes/pkg/scheduler/framework"
	"k8s.io/kubernetes/pkg/scheduler/framework/plugins/feature"
	"k8s.io/kubernetes/pkg/scheduler/framework/runtime"
	st "k8s.io/kubernetes/pkg/scheduler/testing"
)

var (
	podKind = v1.SchemeGroupVersion.WithKind("Pod")

	podName       = "my-pod"
	podUID        = "1234"
	resourceName  = "my-resource"
	resourceName2 = resourceName + "-2"
	claimName     = podName + "-" + resourceName
	claimName2    = podName + "-" + resourceName + "-2"
	className     = "my-resource-class"
	namespace     = "default"

	resourceClass = &resourcev1alpha2.ResourceClass{
		ObjectMeta: metav1.ObjectMeta{
			Name: className,
		},
		DriverName: "some-driver",
	}

	podWithClaimName = st.MakePod().Name(podName).Namespace(namespace).
				UID(podUID).
				PodResourceClaims(v1.PodResourceClaim{Name: resourceName, Source: v1.ClaimSource{ResourceClaimName: &claimName}}).
				Obj()
	otherPodWithClaimName = st.MakePod().Name(podName).Namespace(namespace).
				UID(podUID + "-II").
				PodResourceClaims(v1.PodResourceClaim{Name: resourceName, Source: v1.ClaimSource{ResourceClaimName: &claimName}}).
				Obj()
	podWithClaimTemplate = st.MakePod().Name(podName).Namespace(namespace).
				UID(podUID).
				PodResourceClaims(v1.PodResourceClaim{Name: resourceName, Source: v1.ClaimSource{ResourceClaimTemplateName: &claimName}}).
				Obj()
	podWithTwoClaimNames = st.MakePod().Name(podName).Namespace(namespace).
				UID(podUID).
				PodResourceClaims(v1.PodResourceClaim{Name: resourceName, Source: v1.ClaimSource{ResourceClaimName: &claimName}}).
				PodResourceClaims(v1.PodResourceClaim{Name: resourceName2, Source: v1.ClaimSource{ResourceClaimName: &claimName2}}).
				Obj()

	workerNode = &st.MakeNode().Name("worker").Label("nodename", "worker").Node

	claim = st.MakeResourceClaim().
		Name(claimName).
		Namespace(namespace).
		ResourceClassName(className).
		Obj()
	pendingImmediateClaim = st.FromResourceClaim(claim).
				AllocationMode(resourcev1alpha2.AllocationModeImmediate).
				Obj()
	pendingDelayedClaim = st.FromResourceClaim(claim).
				AllocationMode(resourcev1alpha2.AllocationModeWaitForFirstConsumer).
				Obj()
	pendingDelayedClaim2 = st.FromResourceClaim(pendingDelayedClaim).
				Name(claimName2).
				Obj()
	deallocatingClaim = st.FromResourceClaim(pendingImmediateClaim).
				Allocation(&resourcev1alpha2.AllocationResult{}).
				DeallocationRequested(true).
				Obj()
	inUseClaim = st.FromResourceClaim(pendingImmediateClaim).
			Allocation(&resourcev1alpha2.AllocationResult{}).
			ReservedFor(resourcev1alpha2.ResourceClaimConsumerReference{Resource: "pods", Name: podName, UID: types.UID(podUID)}).
			Obj()
	allocatedClaim = st.FromResourceClaim(pendingDelayedClaim).
			OwnerReference(podName, podUID, podKind).
			Allocation(&resourcev1alpha2.AllocationResult{}).
			Obj()
	allocatedDelayedClaimWithWrongTopology = st.FromResourceClaim(allocatedClaim).
						Allocation(&resourcev1alpha2.AllocationResult{AvailableOnNodes: st.MakeNodeSelector().In("no-such-label", []string{"no-such-value"}).Obj()}).
						Obj()
	allocatedImmediateClaimWithWrongTopology = st.FromResourceClaim(allocatedDelayedClaimWithWrongTopology).
							AllocationMode(resourcev1alpha2.AllocationModeImmediate).
							Obj()
	allocatedClaimWithGoodTopology = st.FromResourceClaim(allocatedClaim).
					Allocation(&resourcev1alpha2.AllocationResult{AvailableOnNodes: st.MakeNodeSelector().In("nodename", []string{"worker"}).Obj()}).
					Obj()
	otherClaim = st.MakeResourceClaim().
			Name("not-my-claim").
			Namespace(namespace).
			ResourceClassName(className).
			Obj()

	scheduling = st.MakePodSchedulingContexts().Name(podName).Namespace(namespace).
			OwnerReference(podName, podUID, podKind).
			Obj()
	schedulingPotential = st.FromPodSchedulingContexts(scheduling).
				PotentialNodes(workerNode.Name).
				Obj()
	schedulingSelectedPotential = st.FromPodSchedulingContexts(schedulingPotential).
					SelectedNode(workerNode.Name).
					Obj()
	schedulingInfo = st.FromPodSchedulingContexts(schedulingPotential).
			ResourceClaims(resourcev1alpha2.ResourceClaimSchedulingStatus{Name: resourceName},
			resourcev1alpha2.ResourceClaimSchedulingStatus{Name: resourceName2}).
		Obj()
)

// result defines the expected outcome of some operation. It covers
// operation's status and the state of the world (= objects).
type result struct {
	status *framework.Status
	// changes contains a mapping of name to an update function for
	// the corresponding object. These functions apply exactly the expected
	// changes to a copy of the object as it existed before the operation.
	changes change

	// added contains objects created by the operation.
	added []metav1.Object

	// removed contains objects deleted by the operation.
	removed []metav1.Object
}

// change contains functions for modifying objects of a certain type. These
// functions will get called for all objects of that type. If they needs to
// make changes only to a particular instance, then it must check the name.
type change struct {
	scheduling func(*resourcev1alpha2.PodSchedulingContext) *resourcev1alpha2.PodSchedulingContext
	claim      func(*resourcev1alpha2.ResourceClaim) *resourcev1alpha2.ResourceClaim
}
type perNodeResult map[string]result

func (p perNodeResult) forNode(nodeName string) result {
	if p == nil {
		return result{}
	}
	return p[nodeName]
}

type want struct {
	preFilterResult  *framework.PreFilterResult
	prefilter        result
	filter           perNodeResult
	prescore         result
	reserve          result
	unreserve        result
	postbind         result
	postFilterResult *framework.PostFilterResult
	postfilter       result
}

// prepare contains changes for objects in the API server.
// Those changes are applied before running the steps. This can
// be used to simulate concurrent changes by some other entities
// like a resource driver.
type prepare struct {
	filter     change
	prescore   change
	reserve    change
	unreserve  change
	postbind   change
	postfilter change
}

func TestPlugin(t *testing.T) {
	testcases := map[string]struct {
		nodes       []*v1.Node // default if unset is workerNode
		pod         *v1.Pod
		claims      []*resourcev1alpha2.ResourceClaim
		classes     []*resourcev1alpha2.ResourceClass
		schedulings []*resourcev1alpha2.PodSchedulingContext

		prepare prepare
		want    want
		disable bool
	}{
		"empty": {
			pod: st.MakePod().Name("foo").Namespace("default").Obj(),
			want: want{
				prefilter: result{
					status: framework.NewStatus(framework.Skip),
				},
				postfilter: result{
					status: framework.NewStatus(framework.Unschedulable, `no new claims to deallocate`),
				},
			},
		},
		"claim-reference": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{allocatedClaim, otherClaim},
			want: want{
				reserve: result{
					changes: change{
						claim: func(claim *resourcev1alpha2.ResourceClaim) *resourcev1alpha2.ResourceClaim {
							if claim.Name == claimName {
								claim = claim.DeepCopy()
								claim.Status.ReservedFor = inUseClaim.Status.ReservedFor
							}
							return claim
						},
					},
				},
			},
		},
		"claim-template": {
			pod:    podWithClaimTemplate,
			claims: []*resourcev1alpha2.ResourceClaim{allocatedClaim, otherClaim},
			want: want{
				reserve: result{
					changes: change{
						claim: func(claim *resourcev1alpha2.ResourceClaim) *resourcev1alpha2.ResourceClaim {
							if claim.Name == claimName {
								claim = claim.DeepCopy()
								claim.Status.ReservedFor = inUseClaim.Status.ReservedFor
							}
							return claim
						},
					},
				},
			},
		},
		"missing-claim": {
			pod: podWithClaimTemplate,
			want: want{
				prefilter: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `waiting for dynamic resource controller to create the resourceclaim "my-pod-my-resource"`),
				},
				postfilter: result{
					status: framework.NewStatus(framework.Unschedulable, `no new claims to deallocate`),
				},
			},
		},
		"waiting-for-immediate-allocation": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{pendingImmediateClaim},
			want: want{
				prefilter: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `unallocated immediate resourceclaim`),
				},
				postfilter: result{
					status: framework.NewStatus(framework.Unschedulable, `no new claims to deallocate`),
				},
			},
		},
		"waiting-for-deallocation": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{deallocatingClaim},
			want: want{
				prefilter: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `resourceclaim must be reallocated`),
				},
				postfilter: result{
					status: framework.NewStatus(framework.Unschedulable, `no new claims to deallocate`),
				},
			},
		},
		"delayed-allocation-missing-class": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{pendingDelayedClaim},
			want: want{
				filter: perNodeResult{
					workerNode.Name: {
						status: framework.AsStatus(fmt.Errorf(`look up resource class: resourceclass.resource.k8s.io "%s" not found`, className)),
					},
				},
				postfilter: result{
					status: framework.NewStatus(framework.Unschedulable, `still not schedulable`),
				},
			},
		},
		"delayed-allocation-scheduling-select-immediately": {
			// Create the PodSchedulingContext object, ask for information
			// and select a node.
			pod:     podWithClaimName,
			claims:  []*resourcev1alpha2.ResourceClaim{pendingDelayedClaim},
			classes: []*resourcev1alpha2.ResourceClass{resourceClass},
			want: want{
				reserve: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `waiting for resource driver to allocate resource`),
					added:  []metav1.Object{schedulingSelectedPotential},
				},
			},
		},
		"delayed-allocation-scheduling-ask": {
			// Create the PodSchedulingContext object, ask for
			// information, but do not select a node because
			// there are multiple claims.
			pod:     podWithTwoClaimNames,
			claims:  []*resourcev1alpha2.ResourceClaim{pendingDelayedClaim, pendingDelayedClaim2},
			classes: []*resourcev1alpha2.ResourceClass{resourceClass},
			want: want{
				reserve: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `waiting for resource driver to provide information`),
					added:  []metav1.Object{schedulingPotential},
				},
			},
		},
		"delayed-allocation-scheduling-finish": {
			// Use the populated PodSchedulingContext object to select a
			// node.
			pod:         podWithClaimName,
			claims:      []*resourcev1alpha2.ResourceClaim{pendingDelayedClaim},
			schedulings: []*resourcev1alpha2.PodSchedulingContext{schedulingInfo},
			classes:     []*resourcev1alpha2.ResourceClass{resourceClass},
			want: want{
				reserve: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `waiting for resource driver to allocate resource`),
					changes: change{
						scheduling: func(in *resourcev1alpha2.PodSchedulingContext) *resourcev1alpha2.PodSchedulingContext {
							return st.FromPodSchedulingContexts(in).
								SelectedNode(workerNode.Name).
								Obj()
						},
					},
				},
			},
		},
		"delayed-allocation-scheduling-finish-concurrent-label-update": {
			// Use the populated PodSchedulingContext object to select a
			// node.
			pod:         podWithClaimName,
			claims:      []*resourcev1alpha2.ResourceClaim{pendingDelayedClaim},
			schedulings: []*resourcev1alpha2.PodSchedulingContext{schedulingInfo},
			classes:     []*resourcev1alpha2.ResourceClass{resourceClass},
			prepare: prepare{
				reserve: change{
					scheduling: func(in *resourcev1alpha2.PodSchedulingContext) *resourcev1alpha2.PodSchedulingContext {
						// This does not actually conflict with setting the
						// selected node, but because the plugin is not using
						// patching yet, Update nonetheless fails.
						return st.FromPodSchedulingContexts(in).
							Label("hello", "world").
							Obj()
					},
				},
			},
			want: want{
				reserve: result{
					status: framework.AsStatus(errors.New(`ResourceVersion must match the object that gets updated`)),
				},
			},
		},
		"delayed-allocation-scheduling-completed": {
			// Remove PodSchedulingContext object once the pod is scheduled.
			pod:         podWithClaimName,
			claims:      []*resourcev1alpha2.ResourceClaim{allocatedClaim},
			schedulings: []*resourcev1alpha2.PodSchedulingContext{schedulingInfo},
			classes:     []*resourcev1alpha2.ResourceClass{resourceClass},
			want: want{
				reserve: result{
					changes: change{
						claim: func(in *resourcev1alpha2.ResourceClaim) *resourcev1alpha2.ResourceClaim {
							return st.FromResourceClaim(in).
								ReservedFor(resourcev1alpha2.ResourceClaimConsumerReference{Resource: "pods", Name: podName, UID: types.UID(podUID)}).
								Obj()
						},
					},
				},
				postbind: result{
					removed: []metav1.Object{schedulingInfo},
				},
			},
		},
		"in-use-by-other": {
			nodes:       []*v1.Node{},
			pod:         otherPodWithClaimName,
			claims:      []*resourcev1alpha2.ResourceClaim{inUseClaim},
			classes:     []*resourcev1alpha2.ResourceClass{},
			schedulings: []*resourcev1alpha2.PodSchedulingContext{},
			prepare:     prepare{},
			want: want{
				prefilter: result{
					status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `resourceclaim in use`),
				},
				postfilter: result{
					status: framework.NewStatus(framework.Unschedulable, `no new claims to deallocate`),
				},
			},
		},
		"wrong-topology-delayed-allocation": {
			// PostFilter tries to get the pod scheduleable by
			// deallocating the claim.
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{allocatedDelayedClaimWithWrongTopology},
			want: want{
				filter: perNodeResult{
					workerNode.Name: {
						status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `resourceclaim not available on the node`),
					},
				},
				postfilter: result{
					// Claims with delayed allocation get deallocated.
					changes: change{
						claim: func(in *resourcev1alpha2.ResourceClaim) *resourcev1alpha2.ResourceClaim {
							return st.FromResourceClaim(in).
								DeallocationRequested(true).
								Obj()
						},
					},
				},
			},
		},
		"wrong-topology-immediate-allocation": {
			// PostFilter tries to get the pod scheduleable by
			// deallocating the claim.
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{allocatedImmediateClaimWithWrongTopology},
			want: want{
				filter: perNodeResult{
					workerNode.Name: {
						status: framework.NewStatus(framework.UnschedulableAndUnresolvable, `resourceclaim not available on the node`),
					},
				},
				postfilter: result{
					// Claims with immediate allocation don't. They would just get allocated again right
					// away, without considering the needs of the pod.
					status: framework.NewStatus(framework.Unschedulable, `still not schedulable`),
				},
			},
		},
		"good-topology": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{allocatedClaimWithGoodTopology},
			want: want{
				reserve: result{
					changes: change{
						claim: func(in *resourcev1alpha2.ResourceClaim) *resourcev1alpha2.ResourceClaim {
							return st.FromResourceClaim(in).
								ReservedFor(resourcev1alpha2.ResourceClaimConsumerReference{Resource: "pods", Name: podName, UID: types.UID(podUID)}).
								Obj()
						},
					},
				},
			},
		},
		"reserved-okay": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{inUseClaim},
		},
		"disable": {
			pod:    podWithClaimName,
			claims: []*resourcev1alpha2.ResourceClaim{inUseClaim},
			want: want{
				prefilter: result{
					status: framework.NewStatus(framework.Skip),
				},
			},
			disable: true,
		},
	}

	for name, tc := range testcases {
		// We can run in parallel because logging is per-test.
		tc := tc
		t.Run(name, func(t *testing.T) {
			t.Parallel()
			nodes := tc.nodes
			if nodes == nil {
				nodes = []*v1.Node{workerNode}
			}
			testCtx := setup(t, nodes, tc.claims, tc.classes, tc.schedulings)
			testCtx.p.enabled = !tc.disable

			initialObjects := testCtx.listAll(t)
			result, status := testCtx.p.PreFilter(testCtx.ctx, testCtx.state, tc.pod)
			t.Run("prefilter", func(t *testing.T) {
				assert.Equal(t, tc.want.preFilterResult, result)
				testCtx.verify(t, tc.want.prefilter, initialObjects, result, status)
			})
			if status.IsSkip() {
				return
			}
			unschedulable := status.Code() != framework.Success

			var potentialNodes []*v1.Node

			initialObjects = testCtx.listAll(t)
			testCtx.updateAPIServer(t, initialObjects, tc.prepare.filter)
			if !unschedulable {
				for _, nodeInfo := range testCtx.nodeInfos {
					initialObjects = testCtx.listAll(t)
					status := testCtx.p.Filter(testCtx.ctx, testCtx.state, tc.pod, nodeInfo)
					nodeName := nodeInfo.Node().Name
					t.Run(fmt.Sprintf("filter/%s", nodeInfo.Node().Name), func(t *testing.T) {
						testCtx.verify(t, tc.want.filter.forNode(nodeName), initialObjects, nil, status)
					})
					if status.Code() != framework.Success {
						unschedulable = true
					} else {
						potentialNodes = append(potentialNodes, nodeInfo.Node())
					}
				}
			}

			if !unschedulable && len(potentialNodes) > 0 {
				initialObjects = testCtx.listAll(t)
				initialObjects = testCtx.updateAPIServer(t, initialObjects, tc.prepare.prescore)
				status := testCtx.p.PreScore(testCtx.ctx, testCtx.state, tc.pod, potentialNodes)
				t.Run("prescore", func(t *testing.T) {
					testCtx.verify(t, tc.want.prescore, initialObjects, nil, status)
				})
				if status.Code() != framework.Success {
					unschedulable = true
				}
			}

			var selectedNode *v1.Node
			if !unschedulable && len(potentialNodes) > 0 {
				selectedNode = potentialNodes[0]

				initialObjects = testCtx.listAll(t)
				initialObjects = testCtx.updateAPIServer(t, initialObjects, tc.prepare.reserve)
				status := testCtx.p.Reserve(testCtx.ctx, testCtx.state, tc.pod, selectedNode.Name)
				t.Run("reserve", func(t *testing.T) {
					testCtx.verify(t, tc.want.reserve, initialObjects, nil, status)
				})
				if status.Code() != framework.Success {
					unschedulable = true
				}
			}

			if selectedNode != nil {
				if unschedulable {
					initialObjects = testCtx.listAll(t)
					initialObjects = testCtx.updateAPIServer(t, initialObjects, tc.prepare.unreserve)
					testCtx.p.Unreserve(testCtx.ctx, testCtx.state, tc.pod, selectedNode.Name)
					t.Run("unreserve", func(t *testing.T) {
						testCtx.verify(t, tc.want.unreserve, initialObjects, nil, status)
					})
				} else {
					initialObjects = testCtx.listAll(t)
					initialObjects = testCtx.updateAPIServer(t, initialObjects, tc.prepare.postbind)
					testCtx.p.PostBind(testCtx.ctx, testCtx.state, tc.pod, selectedNode.Name)
					t.Run("postbind", func(t *testing.T) {
						testCtx.verify(t, tc.want.postbind, initialObjects, nil, status)
					})
				}
			} else {
				initialObjects = testCtx.listAll(t)
				initialObjects = testCtx.updateAPIServer(t, initialObjects, tc.prepare.postfilter)
				result, status := testCtx.p.PostFilter(testCtx.ctx, testCtx.state, tc.pod, nil /* filteredNodeStatusMap not used by plugin */)
				t.Run("postfilter", func(t *testing.T) {
					assert.Equal(t, tc.want.postFilterResult, result)
					testCtx.verify(t, tc.want.postfilter, initialObjects, nil, status)
				})
			}
		})
	}
}

type testContext struct {
	ctx       context.Context
	client    *fake.Clientset
	p         *dynamicResources
	nodeInfos []*framework.NodeInfo
	state     *framework.CycleState
}

func (tc *testContext) verify(t *testing.T, expected result, initialObjects []metav1.Object, result interface{}, status *framework.Status) {
	t.Helper()
	assert.Equal(t, expected.status, status)
	objects := tc.listAll(t)
	wantObjects := update(t, initialObjects, expected.changes)
	for _, add := range expected.added {
		wantObjects = append(wantObjects, add)
	}
	for _, remove := range expected.removed {
		for i, obj := range wantObjects {
			// This is a bit relaxed (no GVR comparison, no UID
			// comparison) to simplify writing the test cases.
			if obj.GetName() == remove.GetName() && obj.GetNamespace() == remove.GetNamespace() {
				wantObjects = append(wantObjects[0:i], wantObjects[i+1:]...)
				break
			}
		}
	}
	sortObjects(wantObjects)
	stripObjects(wantObjects)
	stripObjects(objects)
	assert.Equal(t, wantObjects, objects)
}

// setGVK is implemented by metav1.TypeMeta and thus all API objects, in
// contrast to metav1.Type, which is not (?!) implemented.
type setGVK interface {
	SetGroupVersionKind(gvk schema.GroupVersionKind)
}

// stripObjects removes certain fields (Kind, APIVersion, etc.) which are not
// important and might not be set.
func stripObjects(objects []metav1.Object) {
	for _, obj := range objects {
		obj.SetResourceVersion("")
		obj.SetUID("")
		if objType, ok := obj.(setGVK); ok {
			objType.SetGroupVersionKind(schema.GroupVersionKind{})
		}
	}
}

func (tc *testContext) listAll(t *testing.T) (objects []metav1.Object) {
	t.Helper()
	claims, err := tc.client.ResourceV1alpha2().ResourceClaims("").List(tc.ctx, metav1.ListOptions{})
	require.NoError(t, err, "list claims")
	for _, claim := range claims.Items {
		claim := claim
		objects = append(objects, &claim)
	}
	schedulings, err := tc.client.ResourceV1alpha2().PodSchedulingContexts("").List(tc.ctx, metav1.ListOptions{})
	require.NoError(t, err, "list pod scheduling")
	for _, scheduling := range schedulings.Items {
		scheduling := scheduling
		objects = append(objects, &scheduling)
	}

	sortObjects(objects)
	return
}

// updateAPIServer modifies objects and stores any changed object in the API server.
func (tc *testContext) updateAPIServer(t *testing.T, objects []metav1.Object, updates change) []metav1.Object {
	modified := update(t, objects, updates)
	for i := range modified {
		obj := modified[i]
		if diff := cmp.Diff(objects[i], obj); diff != "" {
			t.Logf("Updating %T %q, diff (-old, +new):\n%s", obj, obj.GetName(), diff)
			switch obj := obj.(type) {
			case *resourcev1alpha2.ResourceClaim:
				obj, err := tc.client.ResourceV1alpha2().ResourceClaims(obj.Namespace).Update(tc.ctx, obj, metav1.UpdateOptions{})
				if err != nil {
					t.Fatalf("unexpected error during prepare update: %v", err)
				}
				modified[i] = obj
			case *resourcev1alpha2.PodSchedulingContext:
				obj, err := tc.client.ResourceV1alpha2().PodSchedulingContexts(obj.Namespace).Update(tc.ctx, obj, metav1.UpdateOptions{})
				if err != nil {
					t.Fatalf("unexpected error during prepare update: %v", err)
				}
				modified[i] = obj
			default:
				t.Fatalf("unsupported object type %T", obj)
			}
		}
	}
	return modified
}

func sortObjects(objects []metav1.Object) {
	sort.Slice(objects, func(i, j int) bool {
		if objects[i].GetNamespace() < objects[j].GetNamespace() {
			return true
		}
		return objects[i].GetName() < objects[j].GetName()
	})
}

// update walks through all existing objects, finds the corresponding update
// function based on name and kind, and replaces those objects that have an
// update function. The rest is left unchanged.
func update(t *testing.T, objects []metav1.Object, updates change) []metav1.Object {
	var updated []metav1.Object

	for _, obj := range objects {
		switch in := obj.(type) {
		case *resourcev1alpha2.ResourceClaim:
			if updates.claim != nil {
				obj = updates.claim(in)
			}
		case *resourcev1alpha2.PodSchedulingContext:
			if updates.scheduling != nil {
				obj = updates.scheduling(in)
			}
		}
		updated = append(updated, obj)
	}

	return updated
}

func setup(t *testing.T, nodes []*v1.Node, claims []*resourcev1alpha2.ResourceClaim, classes []*resourcev1alpha2.ResourceClass, schedulings []*resourcev1alpha2.PodSchedulingContext) (result *testContext) {
	t.Helper()

	tc := &testContext{}
	_, ctx := ktesting.NewTestContext(t)
	ctx, cancel := context.WithCancel(ctx)
	t.Cleanup(cancel)
	tc.ctx = ctx

	tc.client = fake.NewSimpleClientset()
	reactor := createReactor(tc.client.Tracker())
	tc.client.PrependReactor("*", "*", reactor)

	informerFactory := informers.NewSharedInformerFactory(tc.client, 0)

	opts := []runtime.Option{
		runtime.WithClientSet(tc.client),
		runtime.WithInformerFactory(informerFactory),
	}
	fh, err := runtime.NewFramework(ctx, nil, nil, opts...)
	if err != nil {
		t.Fatal(err)
	}

	pl, err := New(nil, fh, feature.Features{EnableDynamicResourceAllocation: true})
	if err != nil {
		t.Fatal(err)
	}
	tc.p = pl.(*dynamicResources)

	// The tests use the API to create the objects because then reactors
	// get triggered.
	for _, claim := range claims {
		_, err := tc.client.ResourceV1alpha2().ResourceClaims(claim.Namespace).Create(tc.ctx, claim, metav1.CreateOptions{})
		require.NoError(t, err, "create resource claim")
	}
	for _, class := range classes {
		_, err := tc.client.ResourceV1alpha2().ResourceClasses().Create(tc.ctx, class, metav1.CreateOptions{})
		require.NoError(t, err, "create resource class")
	}
	for _, scheduling := range schedulings {
		_, err := tc.client.ResourceV1alpha2().PodSchedulingContexts(scheduling.Namespace).Create(tc.ctx, scheduling, metav1.CreateOptions{})
		require.NoError(t, err, "create pod scheduling")
	}

	informerFactory.Start(tc.ctx.Done())
	t.Cleanup(func() {
		// Need to cancel before waiting for the shutdown.
		cancel()
		// Now we can wait for all goroutines to stop.
		informerFactory.Shutdown()
	})

	informerFactory.WaitForCacheSync(tc.ctx.Done())

	for _, node := range nodes {
		nodeInfo := framework.NewNodeInfo()
		nodeInfo.SetNode(node)
		tc.nodeInfos = append(tc.nodeInfos, nodeInfo)
	}
	tc.state = framework.NewCycleState()

	return tc
}

// createReactor implements the logic required for the UID and ResourceVersion
// fields to work when using the fake client. Add it with client.PrependReactor
// to your fake client. ResourceVersion handling is required for conflict
// detection during updates, which is covered by some scenarios.
func createReactor(tracker cgotesting.ObjectTracker) func(action cgotesting.Action) (handled bool, ret apiruntime.Object, err error) {
	var uidCounter int
	var resourceVersionCounter int
	var mutex sync.Mutex

	return func(action cgotesting.Action) (handled bool, ret apiruntime.Object, err error) {
		createAction, ok := action.(cgotesting.CreateAction)
		if !ok {
			return false, nil, nil
		}
		obj, ok := createAction.GetObject().(metav1.Object)
		if !ok {
			return false, nil, nil
		}

		mutex.Lock()
		defer mutex.Unlock()
		switch action.GetVerb() {
		case "create":
			if obj.GetUID() != "" {
				return true, nil, errors.New("UID must not be set on create")
			}
			if obj.GetResourceVersion() != "" {
				return true, nil, errors.New("ResourceVersion must not be set on create")
			}
			obj.SetUID(types.UID(fmt.Sprintf("UID-%d", uidCounter)))
			uidCounter++
			obj.SetResourceVersion(fmt.Sprintf("REV-%d", resourceVersionCounter))
			resourceVersionCounter++
		case "update":
			uid := obj.GetUID()
			resourceVersion := obj.GetResourceVersion()
			if uid == "" {
				return true, nil, errors.New("UID must be set on update")
			}
			if resourceVersion == "" {
				return true, nil, errors.New("ResourceVersion must be set on update")
			}

			oldObj, err := tracker.Get(action.GetResource(), obj.GetNamespace(), obj.GetName())
			if err != nil {
				return true, nil, err
			}
			oldObjMeta, ok := oldObj.(metav1.Object)
			if !ok {
				return true, nil, errors.New("internal error: unexpected old object type")
			}
			if oldObjMeta.GetResourceVersion() != resourceVersion {
				return true, nil, errors.New("ResourceVersion must match the object that gets updated")
			}

			obj.SetResourceVersion(fmt.Sprintf("REV-%d", resourceVersionCounter))
			resourceVersionCounter++
		}
		return false, nil, nil
	}
}
