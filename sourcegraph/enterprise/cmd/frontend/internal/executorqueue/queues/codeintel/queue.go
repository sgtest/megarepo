package codeintel

import (
	"context"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/executorqueue/handler"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/autoindexing"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
	apiclient "github.com/sourcegraph/sourcegraph/enterprise/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

func QueueOptions(observationCtx *observation.Context, db database.DB, accessToken func() (token string, tokenEnabled bool)) handler.QueueOptions[types.Index] {
	recordTransformer := func(ctx context.Context, _ string, record types.Index, resourceMetadata handler.ResourceMetadata) (apiclient.Job, error) {
		return transformRecord(ctx, db, record, resourceMetadata, accessToken)
	}

	store := store.New(observationCtx, db.Handle(), autoindexing.IndexWorkerStoreOptions)

	return handler.QueueOptions[types.Index]{
		Name:              "codeintel",
		Store:             store,
		RecordTransformer: recordTransformer,
	}
}
