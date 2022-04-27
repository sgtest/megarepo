package uploads

import (
	"sync"

	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/log"
)

var (
	svc     *Service
	svcOnce sync.Once
)

// GetService creates or returns an already-initialized uplopads service. If the service is
// new, it will use the given database handle.
func GetService(db database.DB) *Service {
	svcOnce.Do(func() {
		observationContext := &observation.Context{
			Logger:     log.Scoped("uploads.service", "codeintel uploads service"),
			Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
			Registerer: prometheus.DefaultRegisterer,
		}

		svc = newService(
			store.GetStore(db),
			observationContext,
		)
	})

	return svc
}

// TestService creates a fresh uplopads service with the given database handle.
func TestService(db database.DB) *Service {
	return newService(
		store.GetStore(db),
		&observation.TestContext,
	)
}
