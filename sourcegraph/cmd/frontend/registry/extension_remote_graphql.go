package registry

import (
	"context"
	"net/url"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/ui/router"
	"github.com/sourcegraph/sourcegraph/pkg/registry"
)

// registryExtensionRemoteResolver implements the GraphQL type RegistryExtension with data from a
// remote registry.
type registryExtensionRemoteResolver struct {
	v *registry.Extension
}

var _ graphqlbackend.RegistryExtension = &registryExtensionRemoteResolver{}

func (r *registryExtensionRemoteResolver) ID() graphql.ID {
	return MarshalRegistryExtensionID(RegistryExtensionID{
		RemoteID: &registryExtensionRemoteID{Registry: r.v.RegistryURL, UUID: r.v.UUID},
	})
}

// registryExtensionRemoteID identifies a registry extension on a remote registry. It is encoded in
// RegistryExtensionID.
type registryExtensionRemoteID struct {
	Registry string `json:"r"`
	UUID     string `json:"u"`
}

func (r *registryExtensionRemoteResolver) UUID() string { return r.v.UUID }

func (r *registryExtensionRemoteResolver) ExtensionID() string { return r.v.ExtensionID }

func (r *registryExtensionRemoteResolver) ExtensionIDWithoutRegistry() string { return r.v.ExtensionID }

func (r *registryExtensionRemoteResolver) Publisher(ctx context.Context) (graphqlbackend.RegistryPublisher, error) {
	// Remote extensions publisher awareness is not yet implemented.
	return nil, nil
}

func (r *registryExtensionRemoteResolver) Name() string { return r.v.Name }

func (r *registryExtensionRemoteResolver) Manifest(context.Context) (graphqlbackend.ExtensionManifest, error) {
	return NewExtensionManifest(r.v.Manifest), nil
}

func (r *registryExtensionRemoteResolver) CreatedAt() *string {
	return strptr(r.v.CreatedAt.Format(time.RFC3339))
}

func (r *registryExtensionRemoteResolver) UpdatedAt() *string {
	return strptr(r.v.UpdatedAt.Format(time.RFC3339))
}

func (r *registryExtensionRemoteResolver) PublishedAt(context.Context) (*string, error) {
	return strptr(r.v.PublishedAt.Format(time.RFC3339)), nil
}

func (r *registryExtensionRemoteResolver) URL() string {
	return router.Extension(r.v.ExtensionID)
}

func (r *registryExtensionRemoteResolver) RemoteURL() *string {
	return &r.v.URL
}

func (r *registryExtensionRemoteResolver) RegistryName() (string, error) {
	u, err := url.Parse(r.v.RegistryURL)
	if err != nil {
		return "", err
	}
	return registry.Name(u), nil
}

func (r *registryExtensionRemoteResolver) IsLocal() bool { return false }

func (r *registryExtensionRemoteResolver) IsWorkInProgress() bool {
	return IsWorkInProgressExtension(r.v.Manifest)
}

func (r *registryExtensionRemoteResolver) ViewerCanAdminister(ctx context.Context) (bool, error) {
	return false, nil // can't administer remote extensions
}
