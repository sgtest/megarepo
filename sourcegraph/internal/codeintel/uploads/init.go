package uploads

import (
	"sync"

	"github.com/prometheus/client_golang/prometheus"
	"go.opentelemetry.io/otel"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/internal/lsifstore"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/locker"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

var (
	svc     *Service
	svcOnce sync.Once
)

// GetService creates or returns an already-initialized uploads service. If the service is
// new, it will use the given database handle.
func GetService(db, codeIntelDB database.DB, gsc shared.GitserverClient) *Service {
	svcOnce.Do(func() {
		oc := func(name string) *observation.Context {
			return &observation.Context{
				Logger:     log.Scoped("uploads."+name, "codeintel uploads "+name),
				Tracer:     &trace.Tracer{TracerProvider: otel.GetTracerProvider()},
				Registerer: prometheus.DefaultRegisterer,
			}
		}

		lsifstore := lsifstore.New(codeIntelDB, oc("lsifstore"))
		store := store.New(db, oc("store"))
		locker := locker.NewWith(db, "codeintel")

		svc = newService(store, lsifstore, gsc, locker, oc("service"))
	})

	return svc
}
