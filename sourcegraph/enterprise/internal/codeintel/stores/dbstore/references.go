package dbstore

import (
	"context"

	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore"
	"github.com/sourcegraph/sourcegraph/internal/database/batch"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// UpdatePackageReferences inserts reference data tied to the given upload.
func (s *Store) UpdatePackageReferences(ctx context.Context, references []lsifstore.PackageReference) (err error) {
	ctx, endObservation := s.operations.updatePackageReferences.With(ctx, &err, observation.Args{LogFields: []log.Field{}})
	defer endObservation(1, observation.Args{})

	if len(references) == 0 {
		return nil
	}

	inserter := batch.NewBatchInserter(ctx, s.Store.Handle().DB(), "lsif_references", "dump_id", "scheme", "name", "version", "filter")
	for _, r := range references {
		filter := r.Filter
		// avoid not null constraint
		if r.Filter == nil {
			filter = []byte{}
		}

		if err := inserter.Insert(ctx, r.DumpID, r.Scheme, r.Name, r.Version, filter); err != nil {
			return err
		}
	}

	return inserter.Flush(ctx)
}
