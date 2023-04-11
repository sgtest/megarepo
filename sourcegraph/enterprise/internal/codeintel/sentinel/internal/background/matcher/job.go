package matcher

import (
	"context"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/sentinel/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func NewCVEMatcher(store store.Store, observationCtx *observation.Context, config *Config) goroutine.BackgroundRoutine {
	metrics := newMetrics(observationCtx)

	return goroutine.NewPeriodicGoroutine(
		context.Background(),
		"codeintel.sentinel-cve-matcher", "Matches SCIP indexes against known vulnerabilities.",
		config.MatcherInterval,
		goroutine.HandlerFunc(func(ctx context.Context) error {
			numReferencesScanned, numVulnerabilityMatches, err := store.ScanMatches(ctx, config.BatchSize)
			if err != nil {
				return err
			}

			metrics.numReferencesScanned.Add(float64(numReferencesScanned))
			metrics.numVulnerabilityMatches.Add(float64(numVulnerabilityMatches))
			return nil
		}),
	)
}
