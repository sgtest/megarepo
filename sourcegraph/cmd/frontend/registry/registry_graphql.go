package registry

import (
	"context"
	"errors"
	"fmt"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
)

func init() {
	graphqlbackend.ExtensionRegistry = &ExtensionRegistry
}

// ExtensionRegistry is the implementation of the GraphQL types ExtensionRegistry and
// ExtensionRegistryMutation.
//
// To supply implementations of extension registry functionality, use the fields on this value of
// extensionRegistryResolver.
var ExtensionRegistry extensionRegistryResolver

// extensionRegistryResolver implements the GraphQL types ExtensionRegistry and
// ExtensionRegistryMutation.
//
// Some methods are only implemented if there is a local extension registry. For these methods, the
// implementation (if one exists) is set on the XyzFunc struct field.
type extensionRegistryResolver struct {
	ViewerPublishersFunc func(context.Context) ([]graphqlbackend.RegistryPublisher, error)
	PublishersFunc       func(context.Context, *graphqlutil.ConnectionArgs) (graphqlbackend.RegistryPublisherConnection, error)
	CreateExtensionFunc  func(context.Context, *graphqlbackend.ExtensionRegistryCreateExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error)
	UpdateExtensionFunc  func(context.Context, *graphqlbackend.ExtensionRegistryUpdateExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error)
	PublishExtensionFunc func(context.Context, *graphqlbackend.ExtensionRegistryPublishExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error)
	DeleteExtensionFunc  func(context.Context, *graphqlbackend.ExtensionRegistryDeleteExtensionArgs) (*graphqlbackend.EmptyResponse, error)
}

var errNoLocalExtensionRegistry = errors.New("no local extension registry exists")

func (r *extensionRegistryResolver) Publishers(ctx context.Context, args *graphqlutil.ConnectionArgs) (graphqlbackend.RegistryPublisherConnection, error) {
	if r.PublishersFunc == nil {
		return nil, errNoLocalExtensionRegistry
	}
	return r.PublishersFunc(ctx, args)
}

func (r *extensionRegistryResolver) ViewerPublishers(ctx context.Context) ([]graphqlbackend.RegistryPublisher, error) {
	if r.ViewerPublishersFunc == nil {
		return nil, errNoLocalExtensionRegistry
	}
	return r.ViewerPublishersFunc(ctx)
}

func (*extensionRegistryResolver) Extension(ctx context.Context, args *graphqlbackend.ExtensionRegistryExtensionArgs) (graphqlbackend.RegistryExtension, error) {
	return getExtensionByExtensionID(ctx, args.ExtensionID)
}

func getExtensionByExtensionID(ctx context.Context, extensionID string) (graphqlbackend.RegistryExtension, error) {
	local, remote, err := GetExtensionByExtensionID(ctx, extensionID)
	if err != nil {
		return nil, err
	}
	if local != nil {
		return local, nil
	}
	if remote == nil {
		return nil, fmt.Errorf("no remote extension found with ID %q", extensionID)
	}
	return &registryExtensionRemoteResolver{v: remote}, nil
}

func (r *extensionRegistryResolver) CreateExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryCreateExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error) {
	if r.CreateExtensionFunc == nil {
		return nil, errNoLocalExtensionRegistry
	}
	return r.CreateExtensionFunc(ctx, args)
}

func (r *extensionRegistryResolver) UpdateExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryUpdateExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error) {
	if r.UpdateExtensionFunc == nil {
		return nil, errNoLocalExtensionRegistry
	}
	return r.UpdateExtensionFunc(ctx, args)
}

func (r *extensionRegistryResolver) PublishExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryPublishExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error) {
	if r.PublishExtensionFunc == nil {
		return nil, errNoLocalExtensionRegistry
	}
	return r.PublishExtensionFunc(ctx, args)
}

func (r *extensionRegistryResolver) DeleteExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryDeleteExtensionArgs) (*graphqlbackend.EmptyResponse, error) {
	if r.DeleteExtensionFunc == nil {
		return nil, errNoLocalExtensionRegistry
	}
	return r.DeleteExtensionFunc(ctx, args)
}

func (*extensionRegistryResolver) LocalExtensionIDPrefix() *string {
	return GetLocalRegistryExtensionIDPrefix()
}

type ExtensionRegistryMutationResult struct {
	ID int32 // this is only used for local extensions, so it's OK that this only accepts a local extension ID
}

func (r *ExtensionRegistryMutationResult) Extension(ctx context.Context) (graphqlbackend.RegistryExtension, error) {
	return RegistryExtensionByIDInt32(ctx, r.ID)
}
