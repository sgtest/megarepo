package registry

import (
	"context"
	"errors"
	"fmt"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	frontendregistry "github.com/sourcegraph/sourcegraph/cmd/frontend/registry"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
)

func init() {
	frontendregistry.RegistryExtensionByIDInt32 = registryExtensionByIDInt32
	frontendregistry.ExtensionRegistry.CreateExtensionFunc = extensionRegistryCreateExtension
	frontendregistry.ExtensionRegistry.UpdateExtensionFunc = extensionRegistryUpdateExtension
	frontendregistry.ExtensionRegistry.DeleteExtensionFunc = extensionRegistryDeleteExtension
	frontendregistry.ExtensionRegistry.PublishExtensionFunc = extensionRegistryPublishExtension
}

func registryExtensionByIDInt32(ctx context.Context, id int32) (graphqlbackend.RegistryExtension, error) {
	if conf.Extensions() == nil {
		return nil, graphqlbackend.ErrExtensionsDisabled
	}
	x, err := dbExtensions{}.GetByID(ctx, id)
	if err != nil {
		return nil, err
	}
	if err := prefixLocalExtensionID(x); err != nil {
		return nil, err
	}
	return &extensionDBResolver{v: x}, nil
}

func extensionRegistryCreateExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryCreateExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error) {
	if err := licensing.CheckFeature(licensing.FeatureExtensionRegistry); err != nil {
		return nil, err
	}

	publisher, err := unmarshalRegistryPublisherID(args.Publisher)
	if err != nil {
		return nil, err
	}
	// 🚨 SECURITY: Check that the current user can create an extension for this publisher.
	if err := publisher.viewerCanAdminister(ctx); err != nil {
		return nil, err
	}

	// Create the extension.
	id, err := dbExtensions{}.Create(ctx, publisher.userID, publisher.orgID, args.Name)
	if err != nil {
		return nil, err
	}
	return &frontendregistry.ExtensionRegistryMutationResult{ID: id}, nil
}

func viewerCanAdministerExtension(ctx context.Context, id frontendregistry.RegistryExtensionID) error {
	if id.LocalID == 0 {
		return errors.New("unable to administer extension on remote registry")
	}
	extension, err := dbExtensions{}.GetByID(ctx, id.LocalID)
	if err != nil {
		return err
	}
	return toRegistryPublisherID(extension).viewerCanAdminister(ctx)
}

func extensionRegistryUpdateExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryUpdateExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error) {
	id, err := frontendregistry.UnmarshalRegistryExtensionID(args.Extension)
	if err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Check that the current user is authorized to update the extension.
	if err := viewerCanAdministerExtension(ctx, id); err != nil {
		return nil, err
	}

	if err := (dbExtensions{}).Update(ctx, id.LocalID, args.Name); err != nil {
		return nil, err
	}
	return &frontendregistry.ExtensionRegistryMutationResult{ID: id.LocalID}, nil
}

func extensionRegistryDeleteExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryDeleteExtensionArgs) (*graphqlbackend.EmptyResponse, error) {
	id, err := frontendregistry.UnmarshalRegistryExtensionID(args.Extension)
	if err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Check that the current user is authorized to delete the extension.
	if err := viewerCanAdministerExtension(ctx, id); err != nil {
		return nil, err
	}

	if err := (dbExtensions{}).Delete(ctx, id.LocalID); err != nil {
		return nil, err
	}
	return &graphqlbackend.EmptyResponse{}, nil
}

func extensionRegistryPublishExtension(ctx context.Context, args *graphqlbackend.ExtensionRegistryPublishExtensionArgs) (graphqlbackend.ExtensionRegistryMutationResult, error) {
	if err := licensing.CheckFeature(licensing.FeatureExtensionRegistry); err != nil {
		return nil, err
	}

	// Add the prefix if needed, for ease of use.
	configuredPrefix := frontendregistry.GetLocalRegistryExtensionIDPrefix()
	prefix, _, _, err := frontendregistry.SplitExtensionID(args.ExtensionID)
	if err != nil {
		return nil, err
	}
	if prefix == "" && configuredPrefix != nil {
		args.ExtensionID = *configuredPrefix + "/" + args.ExtensionID
	}

	prefix, _, isLocal, err := frontendregistry.ParseExtensionID(args.ExtensionID)
	if err != nil {
		return nil, err
	}
	if !isLocal {
		return nil, fmt.Errorf("unable to publish remote extension %q (publish it directly to the registry on %q)", args.ExtensionID, prefix)
	}

	// Get or create the extension to publish.
	localExtension, _, err := frontendregistry.GetExtensionByExtensionID(ctx, args.ExtensionID)
	if err != nil && !errcode.IsNotFound(err) {
		return nil, err
	}

	// Create the extension if needed.
	var id frontendregistry.RegistryExtensionID
	if localExtension == nil {
		_, publisherName, extensionName, err := frontendregistry.SplitExtensionID(args.ExtensionID)
		if err != nil {
			return nil, err
		}
		publisher, err := dbExtensions{}.GetPublisher(ctx, publisherName)
		if err != nil {
			return nil, err
		}
		publisherID := registryPublisherID{userID: publisher.UserID, orgID: publisher.OrgID}
		// 🚨 SECURITY: Check that the current user can create an extension for this publisher.
		if err := publisherID.viewerCanAdminister(ctx); err != nil {
			return nil, err
		}

		// Create the extension.
		xid, err := dbExtensions{}.Create(ctx, publisherID.userID, publisherID.orgID, extensionName)
		if err != nil {
			return nil, err
		}
		id.LocalID = xid
	} else {
		var err error
		id, err = frontendregistry.UnmarshalRegistryExtensionID(localExtension.ID())
		if err != nil {
			return nil, err
		}
	}

	// 🚨 SECURITY: Check that the current user is authorized to publish the extension.
	if err := viewerCanAdministerExtension(ctx, id); err != nil {
		return nil, err
	}

	// Validate the manifest.
	if err := validateExtensionManifest(args.Manifest); err != nil {
		if !args.Force {
			return nil, fmt.Errorf("invalid extension manifest: %s", err)
		}
	}

	release := dbRelease{
		RegistryExtensionID: id.LocalID,
		CreatorUserID:       actor.FromContext(ctx).UID,
		ReleaseTag:          "release",
		Manifest:            args.Manifest,
		Bundle:              args.Bundle,
		SourceMap:           args.SourceMap,
	}
	if _, err := (dbReleases{}).Create(ctx, &release); err != nil {
		return nil, err
	}
	return &frontendregistry.ExtensionRegistryMutationResult{ID: id.LocalID}, nil
}
