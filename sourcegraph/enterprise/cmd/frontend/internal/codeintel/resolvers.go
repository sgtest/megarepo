package codeintel

import (
	"context"
	"fmt"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	uploadsresolvers "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/transport/graphql"
	resolverstubs "github.com/sourcegraph/sourcegraph/internal/codeintel/resolvers"
)

type Resolver struct {
	autoIndexingRootResolver resolverstubs.AutoindexingServiceResolver
	codenavResolver          resolverstubs.CodeNavServiceResolver
	policiesRootResolver     resolverstubs.PoliciesServiceResolver
	uploadsRootResolver      resolverstubs.UploadsServiceResolver
	sentinelRootResolver     resolverstubs.SentinelServiceResolver
}

func newResolver(
	autoIndexingRootResolver resolverstubs.AutoindexingServiceResolver,
	codenavResolver resolverstubs.CodeNavServiceResolver,
	policiesRootResolver resolverstubs.PoliciesServiceResolver,
	uploadsRootResolver resolverstubs.UploadsServiceResolver,
	sentinelRootResolver resolverstubs.SentinelServiceResolver,
) *Resolver {
	return &Resolver{
		autoIndexingRootResolver: autoIndexingRootResolver,
		codenavResolver:          codenavResolver,
		policiesRootResolver:     policiesRootResolver,
		uploadsRootResolver:      uploadsRootResolver,
		sentinelRootResolver:     sentinelRootResolver,
	}
}

func (r *Resolver) NodeResolvers() map[string]gql.NodeByIDFunc {
	return map[string]gql.NodeByIDFunc{
		"LSIFUpload": func(ctx context.Context, id graphql.ID) (gql.Node, error) {
			uploadID, err := uploadsresolvers.UnmarshalLSIFUploadGQLID(id)
			if err != nil {
				return nil, err
			}

			return r.autoIndexingRootResolver.PreciseIndexByID(ctx, relay.MarshalID("PreciseIndex", fmt.Sprintf("U:%d", uploadID)))
		},
		"CodeIntelligenceConfigurationPolicy": func(ctx context.Context, id graphql.ID) (gql.Node, error) {
			return r.ConfigurationPolicyByID(ctx, id)
		},
		"PreciseIndex": func(ctx context.Context, id graphql.ID) (gql.Node, error) {
			return r.PreciseIndexByID(ctx, id)
		},
		"Vulnerability": func(ctx context.Context, id graphql.ID) (gql.Node, error) {
			return r.VulnerabilityByID(ctx, id)
		},
		"VulnerabilityMatch": func(ctx context.Context, id graphql.ID) (gql.Node, error) {
			return r.VulnerabilityMatchByID(ctx, id)
		},
	}
}

func (r *Resolver) Vulnerabilities(ctx context.Context, args resolverstubs.GetVulnerabilitiesArgs) (_ resolverstubs.VulnerabilityConnectionResolver, err error) {
	return r.sentinelRootResolver.Vulnerabilities(ctx, args)
}

func (r *Resolver) VulnerabilityMatches(ctx context.Context, args resolverstubs.GetVulnerabilityMatchesArgs) (_ resolverstubs.VulnerabilityMatchConnectionResolver, err error) {
	return r.sentinelRootResolver.VulnerabilityMatches(ctx, args)
}

func (r *Resolver) VulnerabilityByID(ctx context.Context, id graphql.ID) (_ resolverstubs.VulnerabilityResolver, err error) {
	return r.sentinelRootResolver.VulnerabilityByID(ctx, id)
}

func (r *Resolver) VulnerabilityMatchByID(ctx context.Context, id graphql.ID) (_ resolverstubs.VulnerabilityMatchResolver, err error) {
	return r.sentinelRootResolver.VulnerabilityMatchByID(ctx, id)
}

func (r *Resolver) IndexerKeys(ctx context.Context, opts *resolverstubs.IndexerKeyQueryArgs) (_ []string, err error) {
	return r.autoIndexingRootResolver.IndexerKeys(ctx, opts)
}

func (r *Resolver) PreciseIndexes(ctx context.Context, args *resolverstubs.PreciseIndexesQueryArgs) (_ resolverstubs.PreciseIndexConnectionResolver, err error) {
	return r.autoIndexingRootResolver.PreciseIndexes(ctx, args)
}

func (r *Resolver) PreciseIndexByID(ctx context.Context, id graphql.ID) (_ resolverstubs.PreciseIndexResolver, err error) {
	return r.autoIndexingRootResolver.PreciseIndexByID(ctx, id)
}

func (r *Resolver) DeletePreciseIndex(ctx context.Context, args *struct{ ID graphql.ID }) (*resolverstubs.EmptyResponse, error) {
	return r.autoIndexingRootResolver.DeletePreciseIndex(ctx, args)
}

func (r *Resolver) DeletePreciseIndexes(ctx context.Context, args *resolverstubs.DeletePreciseIndexesArgs) (*resolverstubs.EmptyResponse, error) {
	return r.autoIndexingRootResolver.DeletePreciseIndexes(ctx, args)
}

func (r *Resolver) ReindexPreciseIndex(ctx context.Context, args *struct{ ID graphql.ID }) (*resolverstubs.EmptyResponse, error) {
	return r.autoIndexingRootResolver.ReindexPreciseIndex(ctx, args)
}

func (r *Resolver) ReindexPreciseIndexes(ctx context.Context, args *resolverstubs.ReindexPreciseIndexesArgs) (*resolverstubs.EmptyResponse, error) {
	return r.autoIndexingRootResolver.ReindexPreciseIndexes(ctx, args)
}

func (r *Resolver) CommitGraph(ctx context.Context, id graphql.ID) (_ resolverstubs.CodeIntelligenceCommitGraphResolver, err error) {
	return r.uploadsRootResolver.CommitGraph(ctx, id)
}

func (r *Resolver) QueueAutoIndexJobsForRepo(ctx context.Context, args *resolverstubs.QueueAutoIndexJobsForRepoArgs) (_ []resolverstubs.PreciseIndexResolver, err error) {
	return r.autoIndexingRootResolver.QueueAutoIndexJobsForRepo(ctx, args)
}

func (r *Resolver) InferAutoIndexJobsForRepo(ctx context.Context, args *resolverstubs.InferAutoIndexJobsForRepoArgs) (_ []resolverstubs.AutoIndexJobDescriptionResolver, err error) {
	return r.autoIndexingRootResolver.InferAutoIndexJobsForRepo(ctx, args)
}

func (r *Resolver) GitBlobLSIFData(ctx context.Context, args *resolverstubs.GitBlobLSIFDataArgs) (_ resolverstubs.GitBlobLSIFDataResolver, err error) {
	return r.codenavResolver.GitBlobLSIFData(ctx, args)
}

func (r *Resolver) GitBlobCodeIntelInfo(ctx context.Context, args *resolverstubs.GitTreeEntryCodeIntelInfoArgs) (_ resolverstubs.GitBlobCodeIntelSupportResolver, err error) {
	return r.autoIndexingRootResolver.GitBlobCodeIntelInfo(ctx, args)
}

func (r *Resolver) GitTreeCodeIntelInfo(ctx context.Context, args *resolverstubs.GitTreeEntryCodeIntelInfoArgs) (resolver resolverstubs.GitTreeCodeIntelSupportResolver, err error) {
	return r.autoIndexingRootResolver.GitTreeCodeIntelInfo(ctx, args)
}

func (r *Resolver) ConfigurationPolicyByID(ctx context.Context, id graphql.ID) (_ resolverstubs.CodeIntelligenceConfigurationPolicyResolver, err error) {
	return r.policiesRootResolver.ConfigurationPolicyByID(ctx, id)
}

func (r *Resolver) CodeIntelligenceConfigurationPolicies(ctx context.Context, args *resolverstubs.CodeIntelligenceConfigurationPoliciesArgs) (_ resolverstubs.CodeIntelligenceConfigurationPolicyConnectionResolver, err error) {
	return r.policiesRootResolver.CodeIntelligenceConfigurationPolicies(ctx, args)
}

func (r *Resolver) CreateCodeIntelligenceConfigurationPolicy(ctx context.Context, args *resolverstubs.CreateCodeIntelligenceConfigurationPolicyArgs) (_ resolverstubs.CodeIntelligenceConfigurationPolicyResolver, err error) {
	return r.policiesRootResolver.CreateCodeIntelligenceConfigurationPolicy(ctx, args)
}

func (r *Resolver) UpdateCodeIntelligenceConfigurationPolicy(ctx context.Context, args *resolverstubs.UpdateCodeIntelligenceConfigurationPolicyArgs) (_ *resolverstubs.EmptyResponse, err error) {
	return r.policiesRootResolver.UpdateCodeIntelligenceConfigurationPolicy(ctx, args)
}

func (r *Resolver) DeleteCodeIntelligenceConfigurationPolicy(ctx context.Context, args *resolverstubs.DeleteCodeIntelligenceConfigurationPolicyArgs) (_ *resolverstubs.EmptyResponse, err error) {
	return r.policiesRootResolver.DeleteCodeIntelligenceConfigurationPolicy(ctx, args)
}

func (r *Resolver) CodeIntelSummary(ctx context.Context) (_ resolverstubs.CodeIntelSummaryResolver, err error) {
	return r.autoIndexingRootResolver.CodeIntelSummary(ctx)
}

func (r *Resolver) RepositorySummary(ctx context.Context, id graphql.ID) (_ resolverstubs.CodeIntelRepositorySummaryResolver, err error) {
	return r.autoIndexingRootResolver.RepositorySummary(ctx, id)
}

func (r *Resolver) IndexConfiguration(ctx context.Context, id graphql.ID) (_ resolverstubs.IndexConfigurationResolver, err error) {
	return r.autoIndexingRootResolver.IndexConfiguration(ctx, id)
}

func (r *Resolver) UpdateRepositoryIndexConfiguration(ctx context.Context, args *resolverstubs.UpdateRepositoryIndexConfigurationArgs) (_ *resolverstubs.EmptyResponse, err error) {
	return r.autoIndexingRootResolver.UpdateRepositoryIndexConfiguration(ctx, args)
}

func (r *Resolver) PreviewRepositoryFilter(ctx context.Context, args *resolverstubs.PreviewRepositoryFilterArgs) (_ resolverstubs.RepositoryFilterPreviewResolver, err error) {
	return r.policiesRootResolver.PreviewRepositoryFilter(ctx, args)
}

func (r *Resolver) CodeIntelligenceInferenceScript(ctx context.Context) (_ string, err error) {
	return r.autoIndexingRootResolver.CodeIntelligenceInferenceScript(ctx)
}

func (r *Resolver) UpdateCodeIntelligenceInferenceScript(ctx context.Context, args *resolverstubs.UpdateCodeIntelligenceInferenceScriptArgs) (_ *resolverstubs.EmptyResponse, err error) {
	return r.autoIndexingRootResolver.UpdateCodeIntelligenceInferenceScript(ctx, args)
}

func (r *Resolver) PreviewGitObjectFilter(ctx context.Context, id graphql.ID, args *resolverstubs.PreviewGitObjectFilterArgs) (_ resolverstubs.GitObjectFilterPreviewResolver, err error) {
	return r.policiesRootResolver.PreviewGitObjectFilter(ctx, id, args)
}
