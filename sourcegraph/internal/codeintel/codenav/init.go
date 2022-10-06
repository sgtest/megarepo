package codenav

import (
	"github.com/sourcegraph/sourcegraph/internal/codeintel/codenav/internal/lsifstore"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/codenav/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/memo"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// GetService creates or returns an already-initialized symbols service.
// If the service is not yet initialized, it will use the provided dependencies.
func GetService(
	db database.DB,
	codeIntelDB stores.CodeIntelDB,
	uploadSvc UploadService,
	gitserver GitserverClient,
) *Service {
	svc, _ := initServiceMemo.Init(serviceDependencies{
		db,
		codeIntelDB,
		uploadSvc,
		gitserver,
	})

	return svc
}

type serviceDependencies struct {
	db          database.DB
	codeIntelDB stores.CodeIntelDB
	uploadSvc   UploadService
	gitserver   GitserverClient
}

var initServiceMemo = memo.NewMemoizedConstructorWithArg(func(deps serviceDependencies) (*Service, error) {
	store := store.New(deps.db, scopedContext("store"))
	lsifStore := lsifstore.New(deps.codeIntelDB, scopedContext("lsifstore"))

	return newService(
		store,
		lsifStore,
		deps.uploadSvc,
		deps.gitserver,
		scopedContext("service"),
	), nil
})

func scopedContext(component string) *observation.Context {
	return observation.ScopedContext("codeintel", "codenav", component)
}
