package resolvers

import (
	"context"
	"fmt"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
)

type RootResolver interface {
	AutoindexingServiceResolver
	CodeNavServiceResolver
	PoliciesServiceResolver
	SentinelServiceResolver
	UploadsServiceResolver
}

type Resolver struct {
	autoIndexingRootResolver AutoindexingServiceResolver
	codenavResolver          CodeNavServiceResolver
	policiesRootResolver     PoliciesServiceResolver
	uploadsRootResolver      UploadsServiceResolver
	sentinelRootResolver     SentinelServiceResolver
}

func NewCodeIntelResolver(
	autoIndexingRootResolver AutoindexingServiceResolver,
	codenavResolver CodeNavServiceResolver,
	policiesRootResolver PoliciesServiceResolver,
	uploadsRootResolver UploadsServiceResolver,
	sentinelRootResolver SentinelServiceResolver,
) *Resolver {
	return &Resolver{
		autoIndexingRootResolver: autoIndexingRootResolver,
		codenavResolver:          codenavResolver,
		policiesRootResolver:     policiesRootResolver,
		uploadsRootResolver:      uploadsRootResolver,
		sentinelRootResolver:     sentinelRootResolver,
	}
}

type (
	Node         interface{ ID() graphql.ID }
	NodeByIDFunc = func(ctx context.Context, id graphql.ID) (Node, error)
)

func (r *Resolver) NodeResolvers() map[string]NodeByIDFunc {
	return map[string]NodeByIDFunc{
		"LSIFUpload": func(ctx context.Context, id graphql.ID) (Node, error) {
			uploadID, err := UnmarshalLSIFUploadGQLID(id)
			if err != nil {
				return nil, err
			}

			return r.autoIndexingRootResolver.PreciseIndexByID(ctx, relay.MarshalID("PreciseIndex", fmt.Sprintf("U:%d", uploadID)))
		},
		"CodeIntelligenceConfigurationPolicy": func(ctx context.Context, id graphql.ID) (Node, error) {
			return r.policiesRootResolver.ConfigurationPolicyByID(ctx, id)
		},
		"PreciseIndex": func(ctx context.Context, id graphql.ID) (Node, error) {
			return r.autoIndexingRootResolver.PreciseIndexByID(ctx, id)
		},
		"Vulnerability": func(ctx context.Context, id graphql.ID) (Node, error) {
			return r.sentinelRootResolver.VulnerabilityByID(ctx, id)
		},
		"VulnerabilityMatch": func(ctx context.Context, id graphql.ID) (Node, error) {
			return r.sentinelRootResolver.VulnerabilityMatchByID(ctx, id)
		},
	}
}

func (r *Resolver) Vulnerabilities(ctx context.Context, args GetVulnerabilitiesArgs) (_ VulnerabilityConnectionResolver, err error) {
	return r.sentinelRootResolver.Vulnerabilities(ctx, args)
}

func (r *Resolver) VulnerabilityMatches(ctx context.Context, args GetVulnerabilityMatchesArgs) (_ VulnerabilityMatchConnectionResolver, err error) {
	return r.sentinelRootResolver.VulnerabilityMatches(ctx, args)
}

func (r *Resolver) VulnerabilityByID(ctx context.Context, id graphql.ID) (_ VulnerabilityResolver, err error) {
	return r.sentinelRootResolver.VulnerabilityByID(ctx, id)
}

func (r *Resolver) VulnerabilityMatchByID(ctx context.Context, id graphql.ID) (_ VulnerabilityMatchResolver, err error) {
	return r.sentinelRootResolver.VulnerabilityMatchByID(ctx, id)
}

func (r *Resolver) IndexerKeys(ctx context.Context, opts *IndexerKeyQueryArgs) (_ []string, err error) {
	return r.autoIndexingRootResolver.IndexerKeys(ctx, opts)
}

func (r *Resolver) PreciseIndexes(ctx context.Context, args *PreciseIndexesQueryArgs) (_ PreciseIndexConnectionResolver, err error) {
	return r.autoIndexingRootResolver.PreciseIndexes(ctx, args)
}

func (r *Resolver) PreciseIndexByID(ctx context.Context, id graphql.ID) (_ PreciseIndexResolver, err error) {
	return r.autoIndexingRootResolver.PreciseIndexByID(ctx, id)
}

func (r *Resolver) DeletePreciseIndex(ctx context.Context, args *struct{ ID graphql.ID }) (*EmptyResponse, error) {
	return r.autoIndexingRootResolver.DeletePreciseIndex(ctx, args)
}

func (r *Resolver) DeletePreciseIndexes(ctx context.Context, args *DeletePreciseIndexesArgs) (*EmptyResponse, error) {
	return r.autoIndexingRootResolver.DeletePreciseIndexes(ctx, args)
}

func (r *Resolver) ReindexPreciseIndex(ctx context.Context, args *struct{ ID graphql.ID }) (*EmptyResponse, error) {
	return r.autoIndexingRootResolver.ReindexPreciseIndex(ctx, args)
}

func (r *Resolver) ReindexPreciseIndexes(ctx context.Context, args *ReindexPreciseIndexesArgs) (*EmptyResponse, error) {
	return r.autoIndexingRootResolver.ReindexPreciseIndexes(ctx, args)
}

func (r *Resolver) CommitGraph(ctx context.Context, id graphql.ID) (_ CodeIntelligenceCommitGraphResolver, err error) {
	return r.uploadsRootResolver.CommitGraph(ctx, id)
}

func (r *Resolver) QueueAutoIndexJobsForRepo(ctx context.Context, args *QueueAutoIndexJobsForRepoArgs) (_ []PreciseIndexResolver, err error) {
	return r.autoIndexingRootResolver.QueueAutoIndexJobsForRepo(ctx, args)
}

func (r *Resolver) InferAutoIndexJobsForRepo(ctx context.Context, args *InferAutoIndexJobsForRepoArgs) (_ []AutoIndexJobDescriptionResolver, err error) {
	return r.autoIndexingRootResolver.InferAutoIndexJobsForRepo(ctx, args)
}

func (r *Resolver) GitBlobLSIFData(ctx context.Context, args *GitBlobLSIFDataArgs) (_ GitBlobLSIFDataResolver, err error) {
	return r.codenavResolver.GitBlobLSIFData(ctx, args)
}

func (r *Resolver) GitBlobCodeIntelInfo(ctx context.Context, args *GitTreeEntryCodeIntelInfoArgs) (_ GitBlobCodeIntelSupportResolver, err error) {
	return r.autoIndexingRootResolver.GitBlobCodeIntelInfo(ctx, args)
}

func (r *Resolver) GitTreeCodeIntelInfo(ctx context.Context, args *GitTreeEntryCodeIntelInfoArgs) (resolver GitTreeCodeIntelSupportResolver, err error) {
	return r.autoIndexingRootResolver.GitTreeCodeIntelInfo(ctx, args)
}

func (r *Resolver) ConfigurationPolicyByID(ctx context.Context, id graphql.ID) (_ CodeIntelligenceConfigurationPolicyResolver, err error) {
	return r.policiesRootResolver.ConfigurationPolicyByID(ctx, id)
}

func (r *Resolver) CodeIntelligenceConfigurationPolicies(ctx context.Context, args *CodeIntelligenceConfigurationPoliciesArgs) (_ CodeIntelligenceConfigurationPolicyConnectionResolver, err error) {
	return r.policiesRootResolver.CodeIntelligenceConfigurationPolicies(ctx, args)
}

func (r *Resolver) CreateCodeIntelligenceConfigurationPolicy(ctx context.Context, args *CreateCodeIntelligenceConfigurationPolicyArgs) (_ CodeIntelligenceConfigurationPolicyResolver, err error) {
	return r.policiesRootResolver.CreateCodeIntelligenceConfigurationPolicy(ctx, args)
}

func (r *Resolver) UpdateCodeIntelligenceConfigurationPolicy(ctx context.Context, args *UpdateCodeIntelligenceConfigurationPolicyArgs) (_ *EmptyResponse, err error) {
	return r.policiesRootResolver.UpdateCodeIntelligenceConfigurationPolicy(ctx, args)
}

func (r *Resolver) DeleteCodeIntelligenceConfigurationPolicy(ctx context.Context, args *DeleteCodeIntelligenceConfigurationPolicyArgs) (_ *EmptyResponse, err error) {
	return r.policiesRootResolver.DeleteCodeIntelligenceConfigurationPolicy(ctx, args)
}

func (r *Resolver) CodeIntelSummary(ctx context.Context) (_ CodeIntelSummaryResolver, err error) {
	return r.autoIndexingRootResolver.CodeIntelSummary(ctx)
}

func (r *Resolver) RepositorySummary(ctx context.Context, id graphql.ID) (_ CodeIntelRepositorySummaryResolver, err error) {
	return r.autoIndexingRootResolver.RepositorySummary(ctx, id)
}

func (r *Resolver) IndexConfiguration(ctx context.Context, id graphql.ID) (_ IndexConfigurationResolver, err error) {
	return r.autoIndexingRootResolver.IndexConfiguration(ctx, id)
}

func (r *Resolver) UpdateRepositoryIndexConfiguration(ctx context.Context, args *UpdateRepositoryIndexConfigurationArgs) (_ *EmptyResponse, err error) {
	return r.autoIndexingRootResolver.UpdateRepositoryIndexConfiguration(ctx, args)
}

func (r *Resolver) PreviewRepositoryFilter(ctx context.Context, args *PreviewRepositoryFilterArgs) (_ RepositoryFilterPreviewResolver, err error) {
	return r.policiesRootResolver.PreviewRepositoryFilter(ctx, args)
}

func (r *Resolver) CodeIntelligenceInferenceScript(ctx context.Context) (_ string, err error) {
	return r.autoIndexingRootResolver.CodeIntelligenceInferenceScript(ctx)
}

func (r *Resolver) UpdateCodeIntelligenceInferenceScript(ctx context.Context, args *UpdateCodeIntelligenceInferenceScriptArgs) (_ *EmptyResponse, err error) {
	return r.autoIndexingRootResolver.UpdateCodeIntelligenceInferenceScript(ctx, args)
}

func (r *Resolver) PreviewGitObjectFilter(ctx context.Context, id graphql.ID, args *PreviewGitObjectFilterArgs) (_ GitObjectFilterPreviewResolver, err error) {
	return r.policiesRootResolver.PreviewGitObjectFilter(ctx, id, args)
}
