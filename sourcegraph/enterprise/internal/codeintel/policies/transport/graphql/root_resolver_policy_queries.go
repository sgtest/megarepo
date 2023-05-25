package graphql

import (
	"context"

	"github.com/graph-gophers/graphql-go"
	"go.opentelemetry.io/otel/attribute"

	policiesshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/policies/shared"
	resolverstubs "github.com/sourcegraph/sourcegraph/internal/codeintel/resolvers"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

const DefaultConfigurationPolicyPageSize = 50

// 🚨 SECURITY: dbstore layer handles authz for GetConfigurationPolicies
func (r *rootResolver) CodeIntelligenceConfigurationPolicies(ctx context.Context, args *resolverstubs.CodeIntelligenceConfigurationPoliciesArgs) (_ resolverstubs.CodeIntelligenceConfigurationPolicyConnectionResolver, err error) {
	ctx, traceErrs, endObservation := r.operations.configurationPolicies.WithErrors(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		attribute.Int("first", int(resolverstubs.Deref(args.First, 0))),
		attribute.String("after", resolverstubs.Deref(args.After, "")),
		attribute.String("repository", string(resolverstubs.Deref(args.Repository, ""))),
		attribute.String("query", resolverstubs.Deref(args.Query, "")),
		attribute.Bool("forDataRetention", resolverstubs.Deref(args.ForDataRetention, false)),
		attribute.Bool("forIndexing", resolverstubs.Deref(args.ForIndexing, false)),
		attribute.Bool("protected", resolverstubs.Deref(args.Protected, false)),
	}})
	endObservation.OnCancel(ctx, 1, observation.Args{})

	limit, offset, err := args.ParseLimitOffset(DefaultConfigurationPolicyPageSize)
	if err != nil {
		return nil, err
	}

	opts := policiesshared.GetConfigurationPoliciesOptions{
		Limit:  int(limit),
		Offset: int(offset),
	}
	if args.Repository != nil {
		id64, err := resolverstubs.UnmarshalID[int64](*args.Repository)
		if err != nil {
			return nil, err
		}
		opts.RepositoryID = int(id64)
	}
	if args.Query != nil {
		opts.Term = *args.Query
	}
	opts.Protected = args.Protected
	opts.ForDataRetention = args.ForDataRetention
	opts.ForIndexing = args.ForIndexing
	opts.ForEmbeddings = args.ForEmbeddings

	configPolicies, totalCount, err := r.policySvc.GetConfigurationPolicies(ctx, opts)
	if err != nil {
		return nil, err
	}

	resolvers := make([]resolverstubs.CodeIntelligenceConfigurationPolicyResolver, 0, len(configPolicies))
	for _, policy := range configPolicies {
		resolvers = append(resolvers, NewConfigurationPolicyResolver(r.repoStore, policy, traceErrs))
	}

	return resolverstubs.NewTotalCountConnectionResolver(resolvers, 0, int32(totalCount)), nil
}

func (r *rootResolver) ConfigurationPolicyByID(ctx context.Context, policyID graphql.ID) (_ resolverstubs.CodeIntelligenceConfigurationPolicyResolver, err error) {
	ctx, traceErrs, endObservation := r.operations.configurationPolicyByID.WithErrors(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		attribute.String("policyID", string(policyID)),
	}})
	endObservation.OnCancel(ctx, 1, observation.Args{})

	configurationPolicyID, err := resolverstubs.UnmarshalID[int](policyID)
	if err != nil {
		return nil, err
	}

	configurationPolicy, exists, err := r.policySvc.GetConfigurationPolicyByID(ctx, configurationPolicyID)
	if err != nil || !exists {
		return nil, err
	}

	return NewConfigurationPolicyResolver(r.repoStore, configurationPolicy, traceErrs), nil
}
