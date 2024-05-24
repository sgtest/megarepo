package graphql

import (
	"context"
	"fmt"
	"strings"
	"sync"

	orderedmap "github.com/wk8/go-ordered-map/v2"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/scip/bindings/go/scip"

	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/codenav"
	resolverstubs "github.com/sourcegraph/sourcegraph/internal/codeintel/resolvers"
	sharedresolvers "github.com/sourcegraph/sourcegraph/internal/codeintel/shared/resolvers"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/shared/resolvers/gitresolvers"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/shared"
	uploadsgraphql "github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/transport/graphql"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/dotcom"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type rootResolver struct {
	svc                            CodeNavService
	autoindexingSvc                AutoIndexingService
	gitserverClient                gitserver.Client
	siteAdminChecker               sharedresolvers.SiteAdminChecker
	repoStore                      database.RepoStore
	uploadLoaderFactory            uploadsgraphql.UploadLoaderFactory
	indexLoaderFactory             uploadsgraphql.IndexLoaderFactory
	locationResolverFactory        *gitresolvers.CachedLocationResolverFactory
	hunkCache                      codenav.HunkCache
	indexResolverFactory           *uploadsgraphql.PreciseIndexResolverFactory
	maximumIndexesPerMonikerSearch int
	operations                     *operations
}

func NewRootResolver(
	observationCtx *observation.Context,
	svc CodeNavService,
	autoindexingSvc AutoIndexingService,
	gitserverClient gitserver.Client,
	siteAdminChecker sharedresolvers.SiteAdminChecker,
	repoStore database.RepoStore,
	uploadLoaderFactory uploadsgraphql.UploadLoaderFactory,
	indexLoaderFactory uploadsgraphql.IndexLoaderFactory,
	indexResolverFactory *uploadsgraphql.PreciseIndexResolverFactory,
	locationResolverFactory *gitresolvers.CachedLocationResolverFactory,
	maxIndexSearch int,
	hunkCacheSize int,
) (resolverstubs.CodeNavServiceResolver, error) {
	hunkCache, err := codenav.NewHunkCache(hunkCacheSize)
	if err != nil {
		return nil, err
	}

	return &rootResolver{
		svc:                            svc,
		autoindexingSvc:                autoindexingSvc,
		gitserverClient:                gitserverClient,
		siteAdminChecker:               siteAdminChecker,
		repoStore:                      repoStore,
		uploadLoaderFactory:            uploadLoaderFactory,
		indexLoaderFactory:             indexLoaderFactory,
		indexResolverFactory:           indexResolverFactory,
		locationResolverFactory:        locationResolverFactory,
		hunkCache:                      hunkCache,
		maximumIndexesPerMonikerSearch: maxIndexSearch,
		operations:                     newOperations(observationCtx),
	}, nil
}

// 🚨 SECURITY: dbstore layer handles authz for query resolution
func (r *rootResolver) GitBlobLSIFData(ctx context.Context, args *resolverstubs.GitBlobLSIFDataArgs) (_ resolverstubs.GitBlobLSIFDataResolver, err error) {
	opts := args.Options()
	ctx, _, endObservation := r.operations.gitBlobLsifData.WithErrors(ctx, &err, observation.Args{Attrs: opts.Attrs()})
	endObservation.OnCancel(ctx, 1, observation.Args{})

	uploads, err := r.svc.GetClosestCompletedUploadsForBlob(ctx, opts)
	if err != nil || len(uploads) == 0 {
		return nil, err
	}

	if len(uploads) == 0 {
		// If we're on sourcegraph.com and it's a rust package repo, index it on-demand
		if dotcom.SourcegraphDotComMode() && strings.HasPrefix(string(args.Repo.Name), "crates/") {
			err = r.autoindexingSvc.QueueRepoRev(ctx, int(args.Repo.ID), string(args.Commit))
		}

		return nil, err
	}

	reqState := codenav.NewRequestState(
		uploads,
		r.repoStore,
		authz.DefaultSubRepoPermsChecker,
		r.gitserverClient,
		args.Repo,
		string(args.Commit),
		args.Path,
		r.maximumIndexesPerMonikerSearch,
		r.hunkCache,
	)

	return newGitBlobLSIFDataResolver(
		r.svc,
		r.indexResolverFactory,
		reqState,
		r.uploadLoaderFactory.Create(),
		r.indexLoaderFactory.Create(),
		r.locationResolverFactory.Create(),
		r.operations,
	), nil
}

func (r *rootResolver) CodeGraphData(ctx context.Context, opts *resolverstubs.CodeGraphDataOpts) (_ *[]resolverstubs.CodeGraphDataResolver, err error) {
	ctx, _, endObservation := r.operations.codeGraphData.WithErrors(ctx, &err, observation.Args{Attrs: opts.Attrs()})
	endObservation.OnCancel(ctx, 1, observation.Args{})

	makeResolvers := func(prov resolverstubs.CodeGraphDataProvenance) ([]resolverstubs.CodeGraphDataResolver, error) {
		indexer := ""
		if prov == resolverstubs.ProvenanceSyntactic {
			indexer = shared.SyntacticIndexer
		}
		uploads, err := r.svc.GetClosestCompletedUploadsForBlob(ctx, shared.UploadMatchingOptions{
			RepositoryID:       int(opts.Repo.ID),
			Commit:             string(opts.Commit),
			Path:               opts.Path,
			RootToPathMatching: shared.RootMustEnclosePath,
			Indexer:            indexer,
		})
		if err != nil || len(uploads) == 0 {
			return nil, err
		}
		resolvers := []resolverstubs.CodeGraphDataResolver{}
		for _, upload := range preferUploadsWithLongestRoots(uploads) {
			resolvers = append(resolvers, newCodeGraphDataResolver(r.svc, upload, opts, prov, r.operations))
		}
		return resolvers, nil
	}

	provs := opts.Args.ProvenancesForSCIPData()
	if provs.Precise {
		preciseResolvers, err := makeResolvers(resolverstubs.ProvenancePrecise)
		if len(preciseResolvers) != 0 || err != nil {
			return &preciseResolvers, err
		}
	}

	if provs.Syntactic {
		syntacticResolvers, err := makeResolvers(resolverstubs.ProvenanceSyntactic)
		if len(syntacticResolvers) != 0 || err != nil {
			return &syntacticResolvers, err
		}

		// Enhancement idea: if a syntactic SCIP index is unavailable,
		// but the language is supported by scip-syntax, we could generate
		// a syntactic SCIP index on-the-fly by having the syntax-highlighter
		// analyze the file.
	}

	// We do not currently have any way of generating SCIP data
	// during purely textual means.

	return &[]resolverstubs.CodeGraphDataResolver{}, nil
}

func preferUploadsWithLongestRoots(uploads []shared.CompletedUpload) []shared.CompletedUpload {
	// Use orderedmap instead of a map to preserve the order of the uploads
	// and to avoid introducing non-determinism.
	sortedMap := orderedmap.New[string, shared.CompletedUpload]()
	for _, upload := range uploads {
		key := fmt.Sprintf("%s:%s", upload.Indexer, upload.Commit)
		if val, found := sortedMap.Get(key); found {
			if len(val.Root) < len(upload.Root) {
				sortedMap.Set(key, upload)
			}
		} else {
			sortedMap.Set(key, upload)
		}
	}
	out := make([]shared.CompletedUpload, 0, sortedMap.Len())
	for pair := sortedMap.Oldest(); pair != nil; pair = pair.Next() {
		out = append(out, pair.Value)
	}
	return out
}

// gitBlobLSIFDataResolver is the main interface to bundle-related operations exposed to the GraphQL API. This
// resolver concerns itself with GraphQL/API-specific behaviors (auth, validation, marshaling, etc.).
// All code intel-specific behavior is delegated to the underlying resolver instance, which is defined
// in the parent package.
type gitBlobLSIFDataResolver struct {
	codeNavSvc           CodeNavService
	indexResolverFactory *uploadsgraphql.PreciseIndexResolverFactory
	requestState         codenav.RequestState
	uploadLoader         uploadsgraphql.UploadLoader
	indexLoader          uploadsgraphql.IndexLoader
	locationResolver     *gitresolvers.CachedLocationResolver
	operations           *operations
}

// NewQueryResolver creates a new QueryResolver with the given resolver that defines all code intel-specific
// behavior. A cached location resolver instance is also given to the query resolver, which should be used
// to resolve all location-related values.
func newGitBlobLSIFDataResolver(
	codeNavSvc CodeNavService,
	indexResolverFactory *uploadsgraphql.PreciseIndexResolverFactory,
	requestState codenav.RequestState,
	uploadLoader uploadsgraphql.UploadLoader,
	indexLoader uploadsgraphql.IndexLoader,
	locationResolver *gitresolvers.CachedLocationResolver,
	operations *operations,
) resolverstubs.GitBlobLSIFDataResolver {
	return &gitBlobLSIFDataResolver{
		codeNavSvc:           codeNavSvc,
		uploadLoader:         uploadLoader,
		indexLoader:          indexLoader,
		indexResolverFactory: indexResolverFactory,
		requestState:         requestState,
		locationResolver:     locationResolver,
		operations:           operations,
	}
}

func (r *gitBlobLSIFDataResolver) ToGitTreeLSIFData() (resolverstubs.GitTreeLSIFDataResolver, bool) {
	return r, true
}

func (r *gitBlobLSIFDataResolver) ToGitBlobLSIFData() (resolverstubs.GitBlobLSIFDataResolver, bool) {
	return r, true
}

func (r *gitBlobLSIFDataResolver) VisibleIndexes(ctx context.Context) (_ *[]resolverstubs.PreciseIndexResolver, err error) {
	ctx, traceErrs, endObservation := r.operations.visibleIndexes.WithErrors(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		attribute.Int("repoID", r.requestState.RepositoryID),
		attribute.String("commit", r.requestState.Commit),
		attribute.String("path", r.requestState.Path),
	}})
	defer endObservation(1, observation.Args{})

	visibleUploads, err := r.codeNavSvc.VisibleUploadsForPath(ctx, r.requestState)
	if err != nil {
		return nil, err
	}

	resolvers := make([]resolverstubs.PreciseIndexResolver, 0, len(visibleUploads))
	for _, u := range visibleUploads {
		upload := u.ConvertToUpload()
		resolver, err := r.indexResolverFactory.Create(
			ctx,
			r.uploadLoader,
			r.indexLoader,
			r.locationResolver,
			traceErrs,
			&upload,
			nil,
		)
		if err != nil {
			return nil, err
		}
		resolvers = append(resolvers, resolver)
	}

	return &resolvers, nil
}

type codeGraphDataResolver struct {
	// Retrieved data/state
	retrievedDocument      sync.Once
	document               *scip.Document
	documentRetrievalError error

	// Arguments
	svc        CodeNavService
	upload     shared.CompletedUpload
	opts       *resolverstubs.CodeGraphDataOpts
	provenance resolverstubs.CodeGraphDataProvenance

	// O11y
	operations *operations
}

func newCodeGraphDataResolver(
	svc CodeNavService,
	upload shared.CompletedUpload,
	opts *resolverstubs.CodeGraphDataOpts,
	provenance resolverstubs.CodeGraphDataProvenance,
	operations *operations,
) resolverstubs.CodeGraphDataResolver {
	return &codeGraphDataResolver{
		sync.Once{},
		/*document*/ nil,
		/*documentRetrievalError*/ nil,
		svc,
		upload,
		opts,
		provenance,
		operations,
	}
}

func (c *codeGraphDataResolver) tryRetrieveDocument(ctx context.Context) (*scip.Document, error) {
	// NOTE(id: scip-doc-optimization): In the case of pagination, if we retrieve the document ID
	// from the database, we can avoid performing a JOIN between codeintel_scip_document_lookup
	// and codeintel_scip_documents
	c.retrievedDocument.Do(func() {
		c.document, c.documentRetrievalError = c.svc.SCIPDocument(ctx, c.upload.ID, c.opts.Path)
	})
	return c.document, c.documentRetrievalError
}

func (c *codeGraphDataResolver) Provenance(_ context.Context) (resolverstubs.CodeGraphDataProvenance, error) {
	return c.provenance, nil
}

func (c *codeGraphDataResolver) Commit(_ context.Context) (string, error) {
	return c.upload.Commit, nil
}

func (c *codeGraphDataResolver) ToolInfo(_ context.Context) (*resolverstubs.CodeGraphToolInfo, error) {
	return &resolverstubs.CodeGraphToolInfo{Name_: &c.upload.Indexer, Version_: &c.upload.IndexerVersion}, nil
}
