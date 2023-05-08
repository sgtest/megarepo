package lsifstore

import (
	"context"

	"github.com/sourcegraph/scip/bindings/go/scip"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/codenav/shared"
	codeintelshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

type LsifStore interface {
	// Whole-document metadata
	GetPathExists(ctx context.Context, bundleID int, path string) (bool, error)
	GetStencil(ctx context.Context, bundleID int, path string) ([]shared.Range, error)
	GetRanges(ctx context.Context, bundleID int, path string, startLine, endLine int) ([]shared.CodeIntelligenceRange, error)

	// Fetch symbol names by position
	GetMonikersByPosition(ctx context.Context, uploadID int, path string, line, character int) ([][]precise.MonikerData, error)
	GetPackageInformation(ctx context.Context, uploadID int, path, packageInformationID string) (precise.PackageInformationData, bool, error)

	// Fetch locations by position
	GetDefinitionLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) ([]shared.Location, int, error)
	GetImplementationLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) ([]shared.Location, int, error)
	GetPrototypeLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) ([]shared.Location, int, error)
	GetReferenceLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) ([]shared.Location, int, error)
	GetBulkMonikerLocations(ctx context.Context, tableName string, uploadIDs []int, monikers []precise.MonikerData, limit, offset int) ([]shared.Location, int, error)

	// Metadata by position
	GetHover(ctx context.Context, bundleID int, path string, line, character int) (string, shared.Range, bool, error)
	GetDiagnostics(ctx context.Context, bundleID int, prefix string, limit, offset int) ([]shared.Diagnostic, int, error)
	SCIPDocument(ctx context.Context, id int, path string) (_ *scip.Document, err error)
}

type store struct {
	db         *basestore.Store
	operations *operations
}

func New(observationCtx *observation.Context, db codeintelshared.CodeIntelDB) LsifStore {
	return &store{
		db:         basestore.NewWithHandle(db.Handle()),
		operations: newOperations(observationCtx),
	}
}
