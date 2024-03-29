package scope

import (
	"fmt"

	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/apimachinery/pkg/runtime/serializer"
	"k8s.io/apiserver/pkg/authorization/authorizer"
	"k8s.io/apiserver/pkg/registry/generic"
	"k8s.io/apiserver/pkg/registry/rest"
	genericapiserver "k8s.io/apiserver/pkg/server"
	"k8s.io/kube-openapi/pkg/common"

	scope "github.com/grafana/grafana/pkg/apis/scope/v0alpha1"
	"github.com/grafana/grafana/pkg/apiserver/builder"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
)

var _ builder.APIGroupBuilder = (*ScopeAPIBuilder)(nil)

// This is used just so wire has something unique to return
type ScopeAPIBuilder struct{}

func NewScopeAPIBuilder() *ScopeAPIBuilder {
	return &ScopeAPIBuilder{}
}

func RegisterAPIService(features featuremgmt.FeatureToggles, apiregistration builder.APIRegistrar) *ScopeAPIBuilder {
	if !features.IsEnabledGlobally(featuremgmt.FlagGrafanaAPIServerWithExperimentalAPIs) {
		return nil // skip registration unless opting into experimental apis
	}
	builder := NewScopeAPIBuilder()
	apiregistration.RegisterAPI(builder)
	return builder
}

func (b *ScopeAPIBuilder) GetAuthorizer() authorizer.Authorizer {
	return nil // default authorizer is fine
}

func (b *ScopeAPIBuilder) GetGroupVersion() schema.GroupVersion {
	return scope.SchemeGroupVersion
}

func (b *ScopeAPIBuilder) InstallSchema(scheme *runtime.Scheme) error {
	err := scope.AddToScheme(scheme)
	if err != nil {
		return err
	}

	err = scheme.AddFieldLabelConversionFunc(
		scope.ScopeResourceInfo.GroupVersionKind(),
		func(label, value string) (string, string, error) {
			fieldSet := SelectableScopeFields(&scope.Scope{})
			for key := range fieldSet {
				if label == key {
					return label, value, nil
				}
			}
			return "", "", fmt.Errorf("field label not supported for %s: %s", scope.ScopeResourceInfo.GroupVersionKind(), label)
		},
	)
	if err != nil {
		return err
	}

	err = scheme.AddFieldLabelConversionFunc(
		scope.ScopeDashboardBindingResourceInfo.GroupVersionKind(),
		func(label, value string) (string, string, error) {
			fieldSet := SelectableScopeDashboardBindingFields(&scope.ScopeDashboardBinding{})
			for key := range fieldSet {
				if label == key {
					return label, value, nil
				}
			}
			return "", "", fmt.Errorf("field label not supported for %s: %s", scope.ScopeDashboardBindingResourceInfo.GroupVersionKind(), label)
		},
	)
	if err != nil {
		return err
	}

	// This is required for --server-side apply
	err = scope.AddKnownTypes(scope.InternalGroupVersion, scheme)
	if err != nil {
		return err
	}

	// Only one version right now
	return scheme.SetVersionPriority(scope.SchemeGroupVersion)
}

func (b *ScopeAPIBuilder) GetAPIGroupInfo(
	scheme *runtime.Scheme,
	codecs serializer.CodecFactory,
	optsGetter generic.RESTOptionsGetter,
	_ bool, // dual write (not relevant)
) (*genericapiserver.APIGroupInfo, error) {
	apiGroupInfo := genericapiserver.NewDefaultAPIGroupInfo(scope.GROUP, scheme, metav1.ParameterCodec, codecs)

	scopeResourceInfo := scope.ScopeResourceInfo
	scopeDashboardResourceInfo := scope.ScopeDashboardBindingResourceInfo

	storage := map[string]rest.Storage{}

	scopeStorage, err := newScopeStorage(scheme, optsGetter)
	if err != nil {
		return nil, err
	}
	storage[scopeResourceInfo.StoragePath()] = scopeStorage

	scopeDashboardStorage, err := newScopeDashboardBindingStorage(scheme, optsGetter)
	if err != nil {
		return nil, err
	}
	storage[scopeDashboardResourceInfo.StoragePath()] = scopeDashboardStorage

	apiGroupInfo.VersionedResourcesStorageMap[scope.VERSION] = storage
	return &apiGroupInfo, nil
}

func (b *ScopeAPIBuilder) GetOpenAPIDefinitions() common.GetOpenAPIDefinitions {
	return scope.GetOpenAPIDefinitions
}

// Register additional routes with the server
func (b *ScopeAPIBuilder) GetAPIRoutes() *builder.APIRoutes {
	return nil
}
