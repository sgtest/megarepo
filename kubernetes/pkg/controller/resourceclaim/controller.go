/*
Copyright 2020 The Kubernetes Authors.

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

package resourceclaim

import (
	"context"
	"fmt"
	"strings"
	"time"

	v1 "k8s.io/api/core/v1"
	resourcev1alpha2 "k8s.io/api/resource/v1alpha2"
	"k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/apimachinery/pkg/util/runtime"
	"k8s.io/apimachinery/pkg/util/wait"
	v1informers "k8s.io/client-go/informers/core/v1"
	resourcev1alpha2informers "k8s.io/client-go/informers/resource/v1alpha2"
	clientset "k8s.io/client-go/kubernetes"
	"k8s.io/client-go/kubernetes/scheme"
	v1core "k8s.io/client-go/kubernetes/typed/core/v1"
	v1listers "k8s.io/client-go/listers/core/v1"
	resourcev1alpha2listers "k8s.io/client-go/listers/resource/v1alpha2"
	"k8s.io/client-go/tools/cache"
	"k8s.io/client-go/tools/record"
	"k8s.io/client-go/util/workqueue"
	"k8s.io/dynamic-resource-allocation/resourceclaim"
	"k8s.io/klog/v2"
	podutil "k8s.io/kubernetes/pkg/api/v1/pod"
	"k8s.io/kubernetes/pkg/controller/resourceclaim/metrics"
	"k8s.io/utils/pointer"
)

const (
	// podResourceClaimIndex is the lookup name for the index function which indexes by pod ResourceClaim templates.
	podResourceClaimIndex = "pod-resource-claim-index"

	maxUIDCacheEntries = 500
)

// Controller creates ResourceClaims for ResourceClaimTemplates in a pod spec.
type Controller struct {
	// kubeClient is the kube API client used to communicate with the API
	// server.
	kubeClient clientset.Interface

	// claimLister is the shared ResourceClaim lister used to fetch and store ResourceClaim
	// objects from the API server. It is shared with other controllers and
	// therefore the ResourceClaim objects in its store should be treated as immutable.
	claimLister  resourcev1alpha2listers.ResourceClaimLister
	claimsSynced cache.InformerSynced

	// podLister is the shared Pod lister used to fetch Pod
	// objects from the API server. It is shared with other controllers and
	// therefore the Pod objects in its store should be treated as immutable.
	podLister v1listers.PodLister
	podSynced cache.InformerSynced

	// templateLister is the shared ResourceClaimTemplate lister used to
	// fetch template objects from the API server. It is shared with other
	// controllers and therefore the objects in its store should be treated
	// as immutable.
	templateLister  resourcev1alpha2listers.ResourceClaimTemplateLister
	templatesSynced cache.InformerSynced

	// podIndexer has the common PodResourceClaim indexer indexer installed To
	// limit iteration over pods to those of interest.
	podIndexer cache.Indexer

	// recorder is used to record events in the API server
	recorder record.EventRecorder

	queue workqueue.RateLimitingInterface

	// The deletedObjects cache keeps track of Pods for which we know that
	// they have existed and have been removed. For those we can be sure
	// that a ReservedFor entry needs to be removed.
	deletedObjects *uidCache
}

const (
	claimKeyPrefix = "claim:"
	podKeyPrefix   = "pod:"
)

// NewController creates a ResourceClaim controller.
func NewController(
	logger klog.Logger,
	kubeClient clientset.Interface,
	podInformer v1informers.PodInformer,
	claimInformer resourcev1alpha2informers.ResourceClaimInformer,
	templateInformer resourcev1alpha2informers.ResourceClaimTemplateInformer) (*Controller, error) {

	ec := &Controller{
		kubeClient:      kubeClient,
		podLister:       podInformer.Lister(),
		podIndexer:      podInformer.Informer().GetIndexer(),
		podSynced:       podInformer.Informer().HasSynced,
		claimLister:     claimInformer.Lister(),
		claimsSynced:    claimInformer.Informer().HasSynced,
		templateLister:  templateInformer.Lister(),
		templatesSynced: templateInformer.Informer().HasSynced,
		queue:           workqueue.NewNamedRateLimitingQueue(workqueue.DefaultControllerRateLimiter(), "resource_claim"),
		deletedObjects:  newUIDCache(maxUIDCacheEntries),
	}

	metrics.RegisterMetrics()

	if _, err := podInformer.Informer().AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc: func(obj interface{}) {
			ec.enqueuePod(logger, obj, false)
		},
		UpdateFunc: func(old, updated interface{}) {
			ec.enqueuePod(logger, updated, false)
		},
		DeleteFunc: func(obj interface{}) {
			ec.enqueuePod(logger, obj, true)
		},
	}); err != nil {
		return nil, err
	}
	if _, err := claimInformer.Informer().AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc: func(obj interface{}) {
			ec.onResourceClaimAddOrUpdate(logger, obj)
		},
		UpdateFunc: func(old, updated interface{}) {
			ec.onResourceClaimAddOrUpdate(logger, updated)
		},
		DeleteFunc: func(obj interface{}) {
			ec.onResourceClaimDelete(logger, obj)
		},
	}); err != nil {
		return nil, err
	}
	if err := ec.podIndexer.AddIndexers(cache.Indexers{podResourceClaimIndex: podResourceClaimIndexFunc}); err != nil {
		return nil, fmt.Errorf("could not initialize ResourceClaim controller: %w", err)
	}

	return ec, nil
}

func (ec *Controller) enqueuePod(logger klog.Logger, obj interface{}, deleted bool) {
	if d, ok := obj.(cache.DeletedFinalStateUnknown); ok {
		obj = d.Obj
	}
	pod, ok := obj.(*v1.Pod)
	if !ok {
		// Not a pod?!
		return
	}

	if deleted {
		ec.deletedObjects.Add(pod.UID)
	}

	if len(pod.Spec.ResourceClaims) == 0 {
		// Nothing to do for it at all.
		return
	}

	logger.V(6).Info("pod with resource claims changed", "pod", klog.KObj(pod), "deleted", deleted)

	// Release reservations of a deleted or completed pod?
	if deleted || isPodDone(pod) {
		for _, podClaim := range pod.Spec.ResourceClaims {
			claimName := resourceclaim.Name(pod, &podClaim)
			key := claimKeyPrefix + pod.Namespace + "/" + claimName
			logger.V(6).Info("pod is deleted or done, process claim", "pod", klog.KObj(pod), "key", key)
			ec.queue.Add(key)
		}
	}

	// Create ResourceClaim for inline templates?
	if pod.DeletionTimestamp == nil {
		for _, podClaim := range pod.Spec.ResourceClaims {
			if podClaim.Source.ResourceClaimTemplateName != nil {
				// It has at least one inline template, work on it.
				key := podKeyPrefix + pod.Namespace + "/" + pod.Name
				logger.V(6).Info("pod is not deleted, process it", "pod", klog.KObj(pod), "key", key)
				ec.queue.Add(key)
				break
			}
		}
	}
}

func (ec *Controller) onResourceClaimAddOrUpdate(logger klog.Logger, obj interface{}) {
	claim, ok := obj.(*resourcev1alpha2.ResourceClaim)
	if !ok {
		return
	}

	// When starting up, we have to check all claims to find those with
	// stale pods in ReservedFor. During an update, a pod might get added
	// that already no longer exists.
	key := claimKeyPrefix + claim.Namespace + "/" + claim.Name
	logger.V(6).Info("claim is new or updated, process it", "key", key)
	ec.queue.Add(key)
}

func (ec *Controller) onResourceClaimDelete(logger klog.Logger, obj interface{}) {
	claim, ok := obj.(*resourcev1alpha2.ResourceClaim)
	if !ok {
		return
	}

	// Someone deleted a ResourceClaim, either intentionally or
	// accidentally. If there is a pod referencing it because of
	// an inline resource, then we should re-create the ResourceClaim.
	// The common indexer does some prefiltering for us by
	// limiting the list to those pods which reference
	// the ResourceClaim.
	objs, err := ec.podIndexer.ByIndex(podResourceClaimIndex, fmt.Sprintf("%s/%s", claim.Namespace, claim.Name))
	if err != nil {
		runtime.HandleError(fmt.Errorf("listing pods from cache: %v", err))
		return
	}
	if len(objs) == 0 {
		logger.V(6).Info("claim got deleted while not needed by any pod, nothing to do", "claim", klog.KObj(claim))
		return
	}
	logger = klog.LoggerWithValues(logger, "claim", klog.KObj(claim))
	for _, obj := range objs {
		ec.enqueuePod(logger, obj, false)
	}
}

func (ec *Controller) Run(ctx context.Context, workers int) {
	defer runtime.HandleCrash()
	defer ec.queue.ShutDown()

	logger := klog.FromContext(ctx)
	logger.Info("Starting ephemeral volume controller")
	defer logger.Info("Shutting down ephemeral volume controller")

	eventBroadcaster := record.NewBroadcaster()
	eventBroadcaster.StartLogging(klog.Infof)
	eventBroadcaster.StartRecordingToSink(&v1core.EventSinkImpl{Interface: ec.kubeClient.CoreV1().Events("")})
	ec.recorder = eventBroadcaster.NewRecorder(scheme.Scheme, v1.EventSource{Component: "resource_claim"})
	defer eventBroadcaster.Shutdown()

	if !cache.WaitForNamedCacheSync("ephemeral", ctx.Done(), ec.podSynced, ec.claimsSynced) {
		return
	}

	for i := 0; i < workers; i++ {
		go wait.UntilWithContext(ctx, ec.runWorker, time.Second)
	}

	<-ctx.Done()
}

func (ec *Controller) runWorker(ctx context.Context) {
	for ec.processNextWorkItem(ctx) {
	}
}

func (ec *Controller) processNextWorkItem(ctx context.Context) bool {
	key, shutdown := ec.queue.Get()
	if shutdown {
		return false
	}
	defer ec.queue.Done(key)

	err := ec.syncHandler(ctx, key.(string))
	if err == nil {
		ec.queue.Forget(key)
		return true
	}

	runtime.HandleError(fmt.Errorf("%v failed with: %v", key, err))
	ec.queue.AddRateLimited(key)

	return true
}

// syncHandler is invoked for each work item which might need to be processed.
// If an error is returned from this function, the item will be requeued.
func (ec *Controller) syncHandler(ctx context.Context, key string) error {
	sep := strings.Index(key, ":")
	if sep < 0 {
		return fmt.Errorf("unexpected key: %s", key)
	}
	prefix, object := key[0:sep+1], key[sep+1:]
	namespace, name, err := cache.SplitMetaNamespaceKey(object)
	if err != nil {
		return err
	}

	switch prefix {
	case podKeyPrefix:
		return ec.syncPod(ctx, namespace, name)
	case claimKeyPrefix:
		return ec.syncClaim(ctx, namespace, name)
	default:
		return fmt.Errorf("unexpected key prefix: %s", prefix)
	}

}

func (ec *Controller) syncPod(ctx context.Context, namespace, name string) error {
	logger := klog.LoggerWithValues(klog.FromContext(ctx), "pod", klog.KRef(namespace, name))
	ctx = klog.NewContext(ctx, logger)
	pod, err := ec.podLister.Pods(namespace).Get(name)
	if err != nil {
		if errors.IsNotFound(err) {
			logger.V(5).Info("nothing to do for pod, it is gone")
			return nil
		}
		return err
	}

	// Ignore pods which are already getting deleted.
	if pod.DeletionTimestamp != nil {
		logger.V(5).Info("nothing to do for pod, it is marked for deletion")
		return nil
	}

	for _, podClaim := range pod.Spec.ResourceClaims {
		if err := ec.handleClaim(ctx, pod, podClaim); err != nil {
			if ec.recorder != nil {
				ec.recorder.Event(pod, v1.EventTypeWarning, "FailedResourceClaimCreation", fmt.Sprintf("PodResourceClaim %s: %v", podClaim.Name, err))
			}
			return fmt.Errorf("pod %s/%s, PodResourceClaim %s: %v", namespace, name, podClaim.Name, err)
		}
	}

	return nil
}

// handleResourceClaim is invoked for each volume of a pod.
func (ec *Controller) handleClaim(ctx context.Context, pod *v1.Pod, podClaim v1.PodResourceClaim) error {
	logger := klog.LoggerWithValues(klog.FromContext(ctx), "podClaim", podClaim.Name)
	ctx = klog.NewContext(ctx, logger)
	logger.V(5).Info("checking", "podClaim", podClaim.Name)
	templateName := podClaim.Source.ResourceClaimTemplateName
	if templateName == nil {
		return nil
	}

	claimName := resourceclaim.Name(pod, &podClaim)
	claim, err := ec.claimLister.ResourceClaims(pod.Namespace).Get(claimName)
	if err != nil && !errors.IsNotFound(err) {
		return err
	}
	if claim != nil {
		if err := resourceclaim.IsForPod(pod, claim); err != nil {
			return err
		}
		// Already created, nothing more to do.
		logger.V(5).Info("claim already created", "podClaim", podClaim.Name, "resourceClaim", claimName)
		return nil
	}

	template, err := ec.templateLister.ResourceClaimTemplates(pod.Namespace).Get(*templateName)
	if err != nil {
		return fmt.Errorf("resource claim template %q: %v", *templateName, err)
	}

	// Create the ResourceClaim with pod as owner.
	isTrue := true
	claim = &resourcev1alpha2.ResourceClaim{
		ObjectMeta: metav1.ObjectMeta{
			Name: claimName,
			OwnerReferences: []metav1.OwnerReference{
				{
					APIVersion:         "v1",
					Kind:               "Pod",
					Name:               pod.Name,
					UID:                pod.UID,
					Controller:         &isTrue,
					BlockOwnerDeletion: &isTrue,
				},
			},
			Annotations: template.Spec.ObjectMeta.Annotations,
			Labels:      template.Spec.ObjectMeta.Labels,
		},
		Spec: template.Spec.Spec,
	}
	metrics.ResourceClaimCreateAttempts.Inc()
	_, err = ec.kubeClient.ResourceV1alpha2().ResourceClaims(pod.Namespace).Create(ctx, claim, metav1.CreateOptions{})
	if err != nil {
		metrics.ResourceClaimCreateFailures.Inc()
		return fmt.Errorf("create ResourceClaim %s: %v", claimName, err)
	}
	return nil
}

func (ec *Controller) syncClaim(ctx context.Context, namespace, name string) error {
	logger := klog.LoggerWithValues(klog.FromContext(ctx), "claim", klog.KRef(namespace, name))
	ctx = klog.NewContext(ctx, logger)
	claim, err := ec.claimLister.ResourceClaims(namespace).Get(name)
	if err != nil {
		if errors.IsNotFound(err) {
			logger.V(5).Info("nothing to do for claim, it is gone")
			return nil
		}
		return err
	}

	// Check if the ReservedFor entries are all still valid.
	valid := make([]resourcev1alpha2.ResourceClaimConsumerReference, 0, len(claim.Status.ReservedFor))
	for _, reservedFor := range claim.Status.ReservedFor {
		if reservedFor.APIGroup == "" &&
			reservedFor.Resource == "pods" {
			// A pod falls into one of three categories:
			// - we have it in our cache -> don't remove it until we are told that it got removed
			// - we don't have it in our cache anymore, but we have seen it before -> it was deleted, remove it
			// - not in our cache, not seen -> double-check with API server before removal

			keepEntry := true

			// Tracking deleted pods in the LRU cache is an
			// optimization. Without this cache, the code would
			// have to do the API call below for every deleted pod
			// to ensure that the pod really doesn't exist. With
			// the cache, most of the time the pod will be recorded
			// as deleted and the API call can be avoided.
			if ec.deletedObjects.Has(reservedFor.UID) {
				// We know that the pod was deleted. This is
				// easy to check and thus is done first.
				keepEntry = false
			} else {
				pod, err := ec.podLister.Pods(claim.Namespace).Get(reservedFor.Name)
				switch {
				case err != nil && !errors.IsNotFound(err):
					return err
				case err != nil:
					// We might not have it in our informer cache
					// yet. Removing the pod while the scheduler is
					// scheduling it would be bad. We have to be
					// absolutely sure and thus have to check with
					// the API server.
					pod, err := ec.kubeClient.CoreV1().Pods(claim.Namespace).Get(ctx, reservedFor.Name, metav1.GetOptions{})
					if err != nil && !errors.IsNotFound(err) {
						return err
					}
					if pod == nil || pod.UID != reservedFor.UID {
						logger.V(6).Info("remove reservation because pod is gone or got replaced", "pod", klog.KObj(pod), "claim", klog.KRef(namespace, name))
						keepEntry = false
					}
				case pod.UID != reservedFor.UID:
					logger.V(6).Info("remove reservation because pod got replaced with new instance", "pod", klog.KObj(pod), "claim", klog.KRef(namespace, name))
					keepEntry = false
				case isPodDone(pod):
					logger.V(6).Info("remove reservation because pod will not run anymore", "pod", klog.KObj(pod), "claim", klog.KRef(namespace, name))
					keepEntry = false
				}
			}

			if keepEntry {
				valid = append(valid, reservedFor)
			}
			continue
		}

		// TODO: support generic object lookup
		return fmt.Errorf("unsupported ReservedFor entry: %v", reservedFor)
	}

	logger.V(5).Info("claim reserved for counts", "currentCount", len(claim.Status.ReservedFor), "claim", klog.KRef(namespace, name), "updatedCount", len(valid))
	if len(valid) < len(claim.Status.ReservedFor) {
		// TODO (#113700): patch
		claim := claim.DeepCopy()
		claim.Status.ReservedFor = valid

		// When a ResourceClaim uses delayed allocation, then it makes sense to
		// deallocate the claim as soon as the last consumer stops using
		// it. This ensures that the claim can be allocated again as needed by
		// some future consumer instead of trying to schedule that consumer
		// onto the node that was chosen for the previous consumer. It also
		// releases the underlying resources for use by other claims.
		//
		// This has to be triggered by the transition from "was being used" to
		// "is not used anymore" because a DRA driver is not required to set
		// `status.reservedFor` together with `status.allocation`, i.e. a claim
		// that is "currently unused" should not get deallocated.
		//
		// This does not matter for claims that were created for a pod. For
		// those, the resource claim controller will trigger deletion when the
		// pod is done. However, it doesn't hurt to also trigger deallocation
		// for such claims and not checking for them keeps this code simpler.
		if len(valid) == 0 &&
			claim.Spec.AllocationMode == resourcev1alpha2.AllocationModeWaitForFirstConsumer {
			claim.Status.DeallocationRequested = true
		}

		_, err := ec.kubeClient.ResourceV1alpha2().ResourceClaims(claim.Namespace).UpdateStatus(ctx, claim, metav1.UpdateOptions{})
		if err != nil {
			return err
		}
	}

	if len(valid) == 0 {
		// Claim is not reserved. If it was generated for a pod and
		// that pod is not going to run, the claim can be
		// deleted. Normally the garbage collector does that, but the
		// pod itself might not get deleted for a while.
		podName, podUID := owningPod(claim)
		if podName != "" {
			pod, err := ec.podLister.Pods(claim.Namespace).Get(podName)
			switch {
			case err == nil:
				// Pod already replaced or not going to run?
				if pod.UID != podUID || isPodDone(pod) {
					// We are certain that the owning pod is not going to need
					// the claim and therefore remove the claim.
					logger.V(5).Info("deleting unused generated claim", "claim", klog.KObj(claim), "pod", klog.KObj(pod))
					err := ec.kubeClient.ResourceV1alpha2().ResourceClaims(claim.Namespace).Delete(ctx, claim.Name, metav1.DeleteOptions{})
					if err != nil {
						return fmt.Errorf("delete claim: %v", err)
					}
				} else {
					logger.V(6).Info("wrong pod content, not deleting claim", "claim", klog.KObj(claim), "podUID", podUID, "podContent", pod)
				}
			case errors.IsNotFound(err):
				// We might not know the pod *yet*. Instead of doing an expensive API call,
				// let the garbage collector handle the case that the pod is truly gone.
				logger.V(5).Info("pod for claim not found", "claim", klog.KObj(claim), "pod", klog.KRef(claim.Namespace, podName))
			default:
				return fmt.Errorf("lookup pod: %v", err)
			}
		} else {
			logger.V(5).Info("claim not generated for a pod", "claim", klog.KObj(claim))
		}
	}

	return nil
}

func owningPod(claim *resourcev1alpha2.ResourceClaim) (string, types.UID) {
	for _, owner := range claim.OwnerReferences {
		if pointer.BoolDeref(owner.Controller, false) &&
			owner.APIVersion == "v1" &&
			owner.Kind == "Pod" {
			return owner.Name, owner.UID
		}
	}
	return "", ""
}

// podResourceClaimIndexFunc is an index function that returns ResourceClaim keys (=
// namespace/name) for ResourceClaimTemplates in a given pod.
func podResourceClaimIndexFunc(obj interface{}) ([]string, error) {
	pod, ok := obj.(*v1.Pod)
	if !ok {
		return []string{}, nil
	}
	keys := []string{}
	for _, podClaim := range pod.Spec.ResourceClaims {
		if podClaim.Source.ResourceClaimTemplateName != nil {
			claimName := resourceclaim.Name(pod, &podClaim)
			keys = append(keys, fmt.Sprintf("%s/%s", pod.Namespace, claimName))
		}
	}
	return keys, nil
}

// isPodDone returns true if it is certain that none of the containers are running and never will run.
func isPodDone(pod *v1.Pod) bool {
	return podutil.IsPodPhaseTerminal(pod.Status.Phase) ||
		// Deleted and not scheduled:
		pod.DeletionTimestamp != nil && pod.Spec.NodeName == ""
}
