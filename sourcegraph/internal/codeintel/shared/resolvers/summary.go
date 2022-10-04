package sharedresolvers

import (
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type CodeIntelRepositorySummaryResolver interface {
	RecentUploads() []LSIFUploadsWithRepositoryNamespaceResolver
	RecentIndexes() []LSIFIndexesWithRepositoryNamespaceResolver
	LastUploadRetentionScan() *DateTime
	LastIndexScan() *DateTime
}

type repositorySummaryResolver struct {
	autoindexingSvc  AutoIndexingService
	uploadsSvc       UploadsService
	policySvc        PolicyService
	summary          RepositorySummary
	prefetcher       *Prefetcher
	locationResolver *CachedLocationResolver
	errTracer        *observation.ErrCollector
}

func NewRepositorySummaryResolver(autoindexingSvc AutoIndexingService, uploadsSvc UploadsService, policySvc PolicyService, summary RepositorySummary, prefetcher *Prefetcher, errTracer *observation.ErrCollector) CodeIntelRepositorySummaryResolver {
	return &repositorySummaryResolver{
		autoindexingSvc:  autoindexingSvc,
		uploadsSvc:       uploadsSvc,
		policySvc:        policySvc,
		summary:          summary,
		prefetcher:       prefetcher,
		locationResolver: NewCachedLocationResolver(autoindexingSvc.GetUnsafeDB()),
		errTracer:        errTracer,
	}
}

func (r *repositorySummaryResolver) RecentUploads() []LSIFUploadsWithRepositoryNamespaceResolver {
	resolvers := make([]LSIFUploadsWithRepositoryNamespaceResolver, 0, len(r.summary.RecentUploads))
	for _, upload := range r.summary.RecentUploads {
		uploadResolvers := make([]LSIFUploadResolver, 0, len(upload.Uploads))
		for _, u := range upload.Uploads {
			uploadResolvers = append(uploadResolvers, NewUploadResolver(r.uploadsSvc, r.autoindexingSvc, r.policySvc, u, r.prefetcher, r.errTracer))
		}

		resolvers = append(resolvers, NewLSIFUploadsWithRepositoryNamespaceResolver(upload, uploadResolvers))
	}

	return resolvers
}

func (r *repositorySummaryResolver) RecentIndexes() []LSIFIndexesWithRepositoryNamespaceResolver {
	resolvers := make([]LSIFIndexesWithRepositoryNamespaceResolver, 0, len(r.summary.RecentIndexes))
	for _, index := range r.summary.RecentIndexes {
		indexResolvers := make([]LSIFIndexResolver, 0, len(index.Indexes))
		for _, idx := range index.Indexes {
			indexResolvers = append(indexResolvers, NewIndexResolver(r.autoindexingSvc, r.uploadsSvc, r.policySvc, idx, r.prefetcher, r.errTracer))
		}
		resolvers = append(resolvers, NewLSIFIndexesWithRepositoryNamespaceResolver(index, indexResolvers))
	}

	return resolvers
}

func (r *repositorySummaryResolver) LastUploadRetentionScan() *DateTime {
	return DateTimeOrNil(r.summary.LastUploadRetentionScan)
}

func (r *repositorySummaryResolver) LastIndexScan() *DateTime {
	return DateTimeOrNil(r.summary.LastIndexScan)
}
