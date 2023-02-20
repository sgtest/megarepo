package lsifstore

import (
	"context"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/codenav/shared"
	codeintelshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

type LsifStore interface {
	// Hover
	GetHover(ctx context.Context, bundleID int, path string, line, character int) (string, types.Range, bool, error)

	// References
	GetReferenceLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) (_ []shared.Location, _ int, err error)

	// Implementation
	GetImplementationLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) (_ []shared.Location, _ int, err error)

	// Definition
	GetDefinitionLocations(ctx context.Context, uploadID int, path string, line, character, limit, offset int) (_ []shared.Location, _ int, err error)

	// Monikers
	GetMonikersByPosition(ctx context.Context, uploadID int, path string, line, character int) (_ [][]precise.MonikerData, err error)
	GetBulkMonikerLocations(ctx context.Context, tableName string, uploadIDs []int, monikers []precise.MonikerData, limit, offset int) (_ []shared.Location, totalCount int, err error)

	// Packages
	GetPackageInformation(ctx context.Context, uploadID int, path, packageInformationID string) (_ precise.PackageInformationData, _ bool, err error)

	// Diagnostics
	GetDiagnostics(ctx context.Context, bundleID int, prefix string, limit, offset int) (_ []shared.Diagnostic, _ int, err error)

	// Stencil
	GetStencil(ctx context.Context, bundleID int, path string) (_ []types.Range, err error)

	// Ranges
	GetRanges(ctx context.Context, bundleID int, path string, startLine, endLine int) (_ []shared.CodeIntelligenceRange, err error)

	// Paths
	GetPathExists(ctx context.Context, bundleID int, path string) (_ bool, err error)
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
