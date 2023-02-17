package graphql

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/autoindexing/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
)

type UploadService interface {
	GetCommitGraphMetadata(ctx context.Context, repositoryID int) (stale bool, updatedAt *time.Time, err error)
	GetAuditLogsForUpload(ctx context.Context, uploadID int) (_ []types.UploadLog, err error)
	GetListTags(ctx context.Context, repo api.RepoName, commitObjs ...string) (_ []*gitdomain.Tag, err error)
	GetUploadDocumentsForPath(ctx context.Context, bundleID int, pathPattern string) (_ []string, _ int, err error)
	GetUploads(ctx context.Context, opts uploadsshared.GetUploadsOptions) (uploads []types.Upload, totalCount int, err error)
	GetUploadsByIDs(ctx context.Context, ids ...int) (_ []types.Upload, err error)
	DeleteUploadByID(ctx context.Context, id int) (_ bool, err error)
	DeleteUploads(ctx context.Context, opts uploadsshared.DeleteUploadsOptions) (err error)
}

type AutoIndexingService interface {
	GetIndexByID(ctx context.Context, id int) (_ types.Index, _ bool, err error)
	GetIndexes(ctx context.Context, opts shared.GetIndexesOptions) (_ []types.Index, _ int, err error)
	GetIndexesByIDs(ctx context.Context, ids ...int) (_ []types.Index, err error)
	GetListTags(ctx context.Context, repo api.RepoName, commitObjs ...string) (_ []*gitdomain.Tag, err error)
	GetUnsafeDB() database.DB

	NumRepositoriesWithCodeIntelligence(ctx context.Context) (int, error)
	RepositoryIDsWithErrors(ctx context.Context, offset, limit int) (_ []shared.RepositoryWithCount, totalCount int, err error)
	RepositoryIDsWithConfiguration(ctx context.Context, offset, limit int) (_ []shared.RepositoryWithAvailableIndexers, totalCount int, err error)
}

type PolicyService interface {
	GetRetentionPolicyOverview(ctx context.Context, upload types.Upload, matchesOnly bool, first int, after int64, query string, now time.Time) (matches []types.RetentionPolicyMatchCandidate, totalCount int, err error)
}
