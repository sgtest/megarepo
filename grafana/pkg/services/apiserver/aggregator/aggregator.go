// SPDX-License-Identifier: AGPL-3.0-only
// Provenance-includes-location: https://github.com/kubernetes/kubernetes/blob/master/cmd/kube-apiserver/app/aggregator.go
// Provenance-includes-license: Apache-2.0
// Provenance-includes-copyright: The Kubernetes Authors.
// Provenance-includes-location: https://github.com/kubernetes/kubernetes/blob/master/cmd/kube-apiserver/app/server.go
// Provenance-includes-license: Apache-2.0
// Provenance-includes-copyright: The Kubernetes Authors.
// Provenance-includes-location: https://github.com/kubernetes/kubernetes/blob/master/pkg/controlplane/apiserver/apiextensions.go
// Provenance-includes-license: Apache-2.0
// Provenance-includes-copyright: The Kubernetes Authors.

package aggregator

import (
	"crypto/tls"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime/schema"
	utilnet "k8s.io/apimachinery/pkg/util/net"
	"k8s.io/apimachinery/pkg/util/sets"
	genericapiserver "k8s.io/apiserver/pkg/server"
	"k8s.io/apiserver/pkg/server/healthz"
	"k8s.io/client-go/informers"
	"k8s.io/client-go/kubernetes/fake"
	"k8s.io/client-go/tools/cache"
	"k8s.io/klog/v2"
	v1 "k8s.io/kube-aggregator/pkg/apis/apiregistration/v1"
	v1helper "k8s.io/kube-aggregator/pkg/apis/apiregistration/v1/helper"
	aggregatorapiserver "k8s.io/kube-aggregator/pkg/apiserver"
	apiregistrationclientset "k8s.io/kube-aggregator/pkg/client/clientset_generated/clientset"
	apiregistrationclient "k8s.io/kube-aggregator/pkg/client/clientset_generated/clientset/typed/apiregistration/v1"
	apiregistrationInformers "k8s.io/kube-aggregator/pkg/client/informers/externalversions/apiregistration/v1"
	"k8s.io/kube-aggregator/pkg/controllers/autoregister"

	serviceclientset "github.com/grafana/grafana/pkg/generated/clientset/versioned"
	informersv0alpha1 "github.com/grafana/grafana/pkg/generated/informers/externalversions"
	"github.com/grafana/grafana/pkg/services/apiserver/options"
)

func CreateAggregatorConfig(commandOptions *options.Options, sharedConfig genericapiserver.RecommendedConfig) (*aggregatorapiserver.Config, informersv0alpha1.SharedInformerFactory, error) {
	// Create a fake clientset and informers for the k8s v1 API group.
	// These are not used in grafana's aggregator because v1 APIs are not available.
	fakev1Informers := informers.NewSharedInformerFactory(fake.NewSimpleClientset(), 10*time.Minute)

	serviceClient, err := serviceclientset.NewForConfig(sharedConfig.LoopbackClientConfig)
	if err != nil {
		return nil, nil, err
	}
	sharedInformerFactory := informersv0alpha1.NewSharedInformerFactory(
		serviceClient,
		5*time.Minute, // this is effectively used as a refresh interval right now.  Might want to do something nicer later on.
	)
	serviceResolver := NewExternalNameResolver(sharedInformerFactory.Service().V0alpha1().ExternalNames().Lister())

	aggregatorConfig := &aggregatorapiserver.Config{
		GenericConfig: &genericapiserver.RecommendedConfig{
			Config:                sharedConfig.Config,
			SharedInformerFactory: fakev1Informers,
			ClientConfig:          sharedConfig.LoopbackClientConfig,
		},
		ExtraConfig: aggregatorapiserver.ExtraConfig{
			ProxyClientCertFile: commandOptions.AggregatorOptions.ProxyClientCertFile,
			ProxyClientKeyFile:  commandOptions.AggregatorOptions.ProxyClientKeyFile,
			// NOTE: while ProxyTransport can be skipped in the configuration, it allows honoring
			// DISABLE_HTTP2, HTTPS_PROXY and NO_PROXY env vars as needed
			ProxyTransport:  createProxyTransport(),
			ServiceResolver: serviceResolver,
		},
	}

	if err := commandOptions.AggregatorOptions.ApplyTo(aggregatorConfig, commandOptions.RecommendedOptions.Etcd, commandOptions.StorageOptions.DataPath); err != nil {
		return nil, nil, err
	}

	return aggregatorConfig, sharedInformerFactory, nil
}

func CreateAggregatorServer(aggregatorConfig *aggregatorapiserver.Config, sharedInformerFactory informersv0alpha1.SharedInformerFactory, delegateAPIServer genericapiserver.DelegationTarget) (*aggregatorapiserver.APIAggregator, error) {
	completedConfig := aggregatorConfig.Complete()
	aggregatorServer, err := completedConfig.NewWithDelegate(delegateAPIServer)
	if err != nil {
		return nil, err
	}

	// create controllers for auto-registration
	apiRegistrationClient, err := apiregistrationclient.NewForConfig(completedConfig.GenericConfig.LoopbackClientConfig)
	if err != nil {
		return nil, err
	}

	autoRegistrationController := autoregister.NewAutoRegisterController(aggregatorServer.APIRegistrationInformers.Apiregistration().V1().APIServices(), apiRegistrationClient)
	apiServices := apiServicesToRegister(delegateAPIServer, autoRegistrationController)

	// Imbue all builtin group-priorities onto the aggregated discovery
	if completedConfig.GenericConfig.AggregatedDiscoveryGroupManager != nil {
		for gv, entry := range APIVersionPriorities {
			completedConfig.GenericConfig.AggregatedDiscoveryGroupManager.SetGroupVersionPriority(metav1.GroupVersion(gv), int(entry.Group), int(entry.Version))
		}
	}

	err = aggregatorServer.GenericAPIServer.AddPostStartHook("grafana-apiserver-autoregistration", func(context genericapiserver.PostStartHookContext) error {
		go autoRegistrationController.Run(5, context.StopCh)
		return nil
	})
	if err != nil {
		return nil, err
	}

	err = aggregatorServer.GenericAPIServer.AddBootSequenceHealthChecks(
		makeAPIServiceAvailableHealthCheck(
			"autoregister-completion",
			apiServices,
			aggregatorServer.APIRegistrationInformers.Apiregistration().V1().APIServices(),
		),
	)
	if err != nil {
		return nil, err
	}

	apiregistrationClient, err := apiregistrationclientset.NewForConfig(completedConfig.GenericConfig.LoopbackClientConfig)
	if err != nil {
		return nil, err
	}

	availableController, err := NewAvailableConditionController(
		aggregatorServer.APIRegistrationInformers.Apiregistration().V1().APIServices(),
		sharedInformerFactory.Service().V0alpha1().ExternalNames(),
		apiregistrationClient.ApiregistrationV1(),
		nil,
		(func() ([]byte, []byte))(nil),
		completedConfig.ExtraConfig.ServiceResolver,
	)
	if err != nil {
		return nil, err
	}

	aggregatorServer.GenericAPIServer.AddPostStartHookOrDie("apiservice-status-override-available-controller", func(context genericapiserver.PostStartHookContext) error {
		// if we end up blocking for long periods of time, we may need to increase workers.
		go availableController.Run(5, context.StopCh)
		return nil
	})

	aggregatorServer.GenericAPIServer.AddPostStartHookOrDie("start-grafana-aggregator-informers", func(context genericapiserver.PostStartHookContext) error {
		sharedInformerFactory.Start(context.StopCh)
		aggregatorServer.APIRegistrationInformers.Start(context.StopCh)
		return nil
	})

	return aggregatorServer, nil
}

func makeAPIService(gv schema.GroupVersion) *v1.APIService {
	apiServicePriority, ok := APIVersionPriorities[gv]
	if !ok {
		// if we aren't found, then we shouldn't register ourselves because it could result in a CRD group version
		// being permanently stuck in the APIServices list.
		klog.Infof("Skipping APIService creation for %v", gv)
		return nil
	}
	return &v1.APIService{
		ObjectMeta: metav1.ObjectMeta{Name: gv.Version + "." + gv.Group},
		Spec: v1.APIServiceSpec{
			Group:                gv.Group,
			Version:              gv.Version,
			GroupPriorityMinimum: apiServicePriority.Group,
			VersionPriority:      apiServicePriority.Version,
		},
	}
}

// makeAPIServiceAvailableHealthCheck returns a healthz check that returns healthy
// once all of the specified services have been observed to be available at least once.
func makeAPIServiceAvailableHealthCheck(name string, apiServices []*v1.APIService, apiServiceInformer apiregistrationInformers.APIServiceInformer) healthz.HealthChecker {
	// Track the auto-registered API services that have not been observed to be available yet
	pendingServiceNamesLock := &sync.RWMutex{}
	pendingServiceNames := sets.NewString()
	for _, service := range apiServices {
		pendingServiceNames.Insert(service.Name)
	}

	// When an APIService in the list is seen as available, remove it from the pending list
	handleAPIServiceChange := func(service *v1.APIService) {
		pendingServiceNamesLock.Lock()
		defer pendingServiceNamesLock.Unlock()
		if !pendingServiceNames.Has(service.Name) {
			return
		}
		if v1helper.IsAPIServiceConditionTrue(service, v1.Available) {
			pendingServiceNames.Delete(service.Name)
		}
	}

	// Watch add/update events for APIServices
	_, err := apiServiceInformer.Informer().AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc:    func(obj interface{}) { handleAPIServiceChange(obj.(*v1.APIService)) },
		UpdateFunc: func(old, new interface{}) { handleAPIServiceChange(new.(*v1.APIService)) },
	})
	if err != nil {
		klog.Errorf("Failed to watch APIServices for health check: %v", err)
	}

	// Don't return healthy until the pending list is empty
	return healthz.NamedCheck(name, func(r *http.Request) error {
		pendingServiceNamesLock.RLock()
		defer pendingServiceNamesLock.RUnlock()
		if pendingServiceNames.Len() > 0 {
			klog.Error("APIServices not yet available", "services", pendingServiceNames.List())
			return fmt.Errorf("missing APIService: %v", pendingServiceNames.List())
		}
		return nil
	})
}

// Priority defines group Priority that is used in discovery. This controls
// group position in the kubectl output.
type Priority struct {
	// Group indicates the order of the Group relative to other groups.
	Group int32
	// Version indicates the relative order of the Version inside of its group.
	Version int32
}

// APIVersionPriorities are the proper way to resolve this letting the aggregator know the desired group and version-within-group order of the underlying servers
// is to refactor the genericapiserver.DelegationTarget to include a list of priorities based on which APIs were installed.
// This requires the APIGroupInfo struct to evolve and include the concept of priorities and to avoid mistakes, the core storage map there needs to be updated.
// That ripples out every bit as far as you'd expect, so for 1.7 we'll include the list here instead of being built up during storage.
var APIVersionPriorities = map[schema.GroupVersion]Priority{
	{Group: "", Version: "v1"}: {Group: 18000, Version: 1},
	// to my knowledge, nothing below here collides
	{Group: "admissionregistration.k8s.io", Version: "v1"}:       {Group: 16700, Version: 15},
	{Group: "admissionregistration.k8s.io", Version: "v1beta1"}:  {Group: 16700, Version: 12},
	{Group: "admissionregistration.k8s.io", Version: "v1alpha1"}: {Group: 16700, Version: 9},
	// Append a new group to the end of the list if unsure.
	// You can use min(existing group)-100 as the initial value for a group.
	// Version can be set to 9 (to have space around) for a new group.
}

func apiServicesToRegister(delegateAPIServer genericapiserver.DelegationTarget, registration autoregister.AutoAPIServiceRegistration) []*v1.APIService {
	apiServices := []*v1.APIService{}

	for _, curr := range delegateAPIServer.ListedPaths() {
		if curr == "/api/v1" {
			apiService := makeAPIService(schema.GroupVersion{Group: "", Version: "v1"})
			registration.AddAPIServiceToSyncOnStart(apiService)
			apiServices = append(apiServices, apiService)
			continue
		}

		if !strings.HasPrefix(curr, "/apis/") {
			continue
		}
		// this comes back in a list that looks like /apis/rbac.authorization.k8s.io/v1alpha1
		tokens := strings.Split(curr, "/")
		if len(tokens) != 4 {
			continue
		}

		apiService := makeAPIService(schema.GroupVersion{Group: tokens[2], Version: tokens[3]})
		if apiService == nil {
			continue
		}
		registration.AddAPIServiceToSyncOnStart(apiService)
		apiServices = append(apiServices, apiService)
	}

	return apiServices
}

// NOTE: below function imported from https://github.com/kubernetes/kubernetes/blob/master/cmd/kube-apiserver/app/server.go#L197
// createProxyTransport creates the dialer infrastructure to connect to the api servers.
func createProxyTransport() *http.Transport {
	// NOTE: We don't set proxyDialerFn but the below SetTransportDefaults will
	// See https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apimachinery/pkg/util/net/http.go#L109
	var proxyDialerFn utilnet.DialFunc
	// Proxying to services is IP-based... don't expect to be able to verify the hostname
	proxyTLSClientConfig := &tls.Config{InsecureSkipVerify: true}
	proxyTransport := utilnet.SetTransportDefaults(&http.Transport{
		DialContext:     proxyDialerFn,
		TLSClientConfig: proxyTLSClientConfig,
	})
	return proxyTransport
}
