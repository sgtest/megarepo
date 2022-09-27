package resolvers

import (
	"context"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/externallink"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const workspaceFileIDKind = "BatchSpecWorkspaceFile"

func marshalWorkspaceFileRandID(id string) graphql.ID {
	return relay.MarshalID(workspaceFileIDKind, id)
}

var _ graphqlbackend.BatchWorkspaceFileResolver = &batchSpecWorkspaceFileResolver{}

type batchSpecWorkspaceFileResolver struct {
	batchSpecRandID string
	file            *btypes.BatchSpecWorkspaceFile
}

func (r *batchSpecWorkspaceFileResolver) ID() graphql.ID {
	// 🚨 SECURITY: This needs to be the RandID! We can't expose the
	// sequential, guessable ID.
	return marshalWorkspaceFileRandID(r.file.RandID)
}

func (r *batchSpecWorkspaceFileResolver) ModifiedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: r.file.ModifiedAt}
}

func (r *batchSpecWorkspaceFileResolver) CreatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: r.file.CreatedAt}
}

func (r *batchSpecWorkspaceFileResolver) UpdatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: r.file.UpdatedAt}
}

func (r *batchSpecWorkspaceFileResolver) Name() string {
	return r.file.FileName
}

func (r *batchSpecWorkspaceFileResolver) Path() string {
	return r.file.Path
}

func (r *batchSpecWorkspaceFileResolver) IsDirectory() bool {
	// A workspace file cannot be a directory.
	return false
}

func (r *batchSpecWorkspaceFileResolver) Content(ctx context.Context) (string, error) {
	return "", errors.New("not implemented")
}

func (r *batchSpecWorkspaceFileResolver) ByteSize(ctx context.Context) (int32, error) {
	return int32(r.file.Size), nil
}

func (r *batchSpecWorkspaceFileResolver) Binary(ctx context.Context) (bool, error) {
	return false, errors.New("not implemented")
}

func (r *batchSpecWorkspaceFileResolver) RichHTML(ctx context.Context) (string, error) {
	return "", errors.New("not implemented")
}

func (r *batchSpecWorkspaceFileResolver) URL(ctx context.Context) (string, error) {
	return "", errors.New("not implemented")
}

func (r *batchSpecWorkspaceFileResolver) CanonicalURL() string {
	return ""
}

func (r *batchSpecWorkspaceFileResolver) ExternalURLs(ctx context.Context) ([]*externallink.Resolver, error) {
	return nil, errors.New("not implemented")
}

func (r *batchSpecWorkspaceFileResolver) Highlight(ctx context.Context, args *graphqlbackend.HighlightArgs) (*graphqlbackend.HighlightedFileResolver, error) {
	return nil, errors.New("not implemented")
}

func (r *batchSpecWorkspaceFileResolver) ToGitBlob() (*graphqlbackend.GitTreeEntryResolver, bool) {
	return nil, false
}

func (r *batchSpecWorkspaceFileResolver) ToVirtualFile() (*graphqlbackend.VirtualFileResolver, bool) {
	return nil, false
}

func (r *batchSpecWorkspaceFileResolver) ToBatchSpecWorkspaceFile() (graphqlbackend.BatchWorkspaceFileResolver, bool) {
	return r, true
}
