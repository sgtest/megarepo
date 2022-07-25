package graphqlbackend

import (
	"context"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	executor "github.com/sourcegraph/sourcegraph/internal/services/executors/transport/graphql"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Node must be implemented by any resolver that implements the Node interface in
// GraphQL. When defining a new type implementing Node, the NodeResolver below
// needs a ToXX type assertion method, and the node resolver needs to be registered
// in the nodeByIDFns on the schemaResolver.
type Node interface {
	ID() graphql.ID
}

func (r *schemaResolver) Node(ctx context.Context, args *struct{ ID graphql.ID }) (*NodeResolver, error) {
	kind := relay.UnmarshalKind(args.ID)
	nodeRes, ok := r.nodeByIDFns[kind]
	if !ok {
		return nil, errors.New("invalid id")
	}
	n, err := nodeRes(ctx, args.ID)
	if err != nil {
		return nil, err
	}
	if n == nil {
		return nil, nil
	}
	return &NodeResolver{n}, nil
}

type NodeByIDFunc = func(ctx context.Context, id graphql.ID) (Node, error)

func (r *schemaResolver) nodeByID(ctx context.Context, id graphql.ID) (Node, error) {
	kind := relay.UnmarshalKind(id)
	nodeRes, ok := r.nodeByIDFns[kind]
	if !ok {
		return nil, errors.New("invalid id")
	}
	return nodeRes(ctx, id)
}

type NodeResolver struct {
	Node
}

func (r *NodeResolver) ToAccessToken() (*accessTokenResolver, bool) {
	n, ok := r.Node.(*accessTokenResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitor() (MonitorResolver, bool) {
	n, ok := r.Node.(MonitorResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitorQuery() (MonitorQueryResolver, bool) {
	n, ok := r.Node.(MonitorQueryResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitorEmail() (MonitorEmailResolver, bool) {
	n, ok := r.Node.(MonitorEmailResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitorWebhook() (MonitorWebhookResolver, bool) {
	n, ok := r.Node.(MonitorWebhookResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitorSlackWebhook() (MonitorSlackWebhookResolver, bool) {
	n, ok := r.Node.(MonitorSlackWebhookResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitorActionEvent() (MonitorActionEventResolver, bool) {
	n, ok := r.Node.(MonitorActionEventResolver)
	return n, ok
}

func (r *NodeResolver) ToMonitorTriggerEvent() (MonitorTriggerEventResolver, bool) {
	n, ok := r.Node.(MonitorTriggerEventResolver)
	return n, ok
}

func (r *NodeResolver) ToBatchChange() (BatchChangeResolver, bool) {
	n, ok := r.Node.(BatchChangeResolver)
	return n, ok
}

func (r *NodeResolver) ToBatchSpec() (BatchSpecResolver, bool) {
	n, ok := r.Node.(BatchSpecResolver)
	return n, ok
}

func (r *NodeResolver) ToExternalChangeset() (ExternalChangesetResolver, bool) {
	n, ok := r.Node.(ChangesetResolver)
	if !ok {
		return nil, false
	}
	return n.ToExternalChangeset()
}

func (r *NodeResolver) ToHiddenExternalChangeset() (HiddenExternalChangesetResolver, bool) {
	n, ok := r.Node.(ChangesetResolver)
	if !ok {
		return nil, false
	}
	return n.ToHiddenExternalChangeset()
}

func (r *NodeResolver) ToChangesetEvent() (ChangesetEventResolver, bool) {
	n, ok := r.Node.(ChangesetEventResolver)
	return n, ok
}

func (r *NodeResolver) ToHiddenChangesetSpec() (HiddenChangesetSpecResolver, bool) {
	n, ok := r.Node.(ChangesetSpecResolver)
	if !ok {
		return nil, ok
	}
	return n.ToHiddenChangesetSpec()
}

func (r *NodeResolver) ToVisibleChangesetSpec() (VisibleChangesetSpecResolver, bool) {
	n, ok := r.Node.(ChangesetSpecResolver)
	if !ok {
		return nil, ok
	}
	return n.ToVisibleChangesetSpec()
}

func (r *NodeResolver) ToBatchChangesCredential() (BatchChangesCredentialResolver, bool) {
	n, ok := r.Node.(BatchChangesCredentialResolver)
	return n, ok
}

func (r *NodeResolver) ToProductLicense() (ProductLicense, bool) {
	n, ok := r.Node.(ProductLicense)
	return n, ok
}

func (r *NodeResolver) ToProductSubscription() (ProductSubscription, bool) {
	n, ok := r.Node.(ProductSubscription)
	return n, ok
}

func (r *NodeResolver) ToExternalAccount() (*externalAccountResolver, bool) {
	n, ok := r.Node.(*externalAccountResolver)
	return n, ok
}

func (r *NodeResolver) ToExternalService() (*externalServiceResolver, bool) {
	n, ok := r.Node.(*externalServiceResolver)
	return n, ok
}

func (r *NodeResolver) ToGitRef() (*GitRefResolver, bool) {
	n, ok := r.Node.(*GitRefResolver)
	return n, ok
}

func (r *NodeResolver) ToRepository() (*RepositoryResolver, bool) {
	n, ok := r.Node.(*RepositoryResolver)
	return n, ok
}

func (r *NodeResolver) ToUser() (*UserResolver, bool) {
	n, ok := r.Node.(*UserResolver)
	return n, ok
}

func (r *NodeResolver) ToOrg() (*OrgResolver, bool) {
	n, ok := r.Node.(*OrgResolver)
	return n, ok
}

func (r *NodeResolver) ToOrganizationInvitation() (*organizationInvitationResolver, bool) {
	n, ok := r.Node.(*organizationInvitationResolver)
	return n, ok
}

func (r *NodeResolver) ToGitCommit() (*GitCommitResolver, bool) {
	n, ok := r.Node.(*GitCommitResolver)
	return n, ok
}

func (r *NodeResolver) ToRegistryExtension() (RegistryExtension, bool) {
	if NodeToRegistryExtension == nil {
		return nil, false
	}
	return NodeToRegistryExtension(r.Node)
}

func (r *NodeResolver) ToSavedSearch() (*savedSearchResolver, bool) {
	n, ok := r.Node.(*savedSearchResolver)
	return n, ok
}

func (r *NodeResolver) ToSearchContext() (SearchContextResolver, bool) {
	n, ok := r.Node.(SearchContextResolver)
	return n, ok
}

func (r *NodeResolver) ToNotebook() (NotebookResolver, bool) {
	n, ok := r.Node.(NotebookResolver)
	return n, ok
}

func (r *NodeResolver) ToSite() (*siteResolver, bool) {
	n, ok := r.Node.(*siteResolver)
	return n, ok
}

func (r *NodeResolver) ToLSIFUpload() (LSIFUploadResolver, bool) {
	n, ok := r.Node.(LSIFUploadResolver)
	return n, ok
}

func (r *NodeResolver) ToLSIFIndex() (LSIFIndexResolver, bool) {
	n, ok := r.Node.(LSIFIndexResolver)
	return n, ok
}

func (r *NodeResolver) ToCodeIntelligenceConfigurationPolicy() (CodeIntelligenceConfigurationPolicyResolver, bool) {
	n, ok := r.Node.(CodeIntelligenceConfigurationPolicyResolver)
	return n, ok
}

func (r *NodeResolver) ToOutOfBandMigration() (*outOfBandMigrationResolver, bool) {
	n, ok := r.Node.(*outOfBandMigrationResolver)
	return n, ok
}

func (r *NodeResolver) ToBulkOperation() (BulkOperationResolver, bool) {
	n, ok := r.Node.(BulkOperationResolver)
	return n, ok
}

func (r *NodeResolver) ToHiddenBatchSpecWorkspace() (HiddenBatchSpecWorkspaceResolver, bool) {
	n, ok := r.Node.(BatchSpecWorkspaceResolver)
	if !ok {
		return nil, ok
	}
	return n.ToHiddenBatchSpecWorkspace()
}

func (r *NodeResolver) ToVisibleBatchSpecWorkspace() (VisibleBatchSpecWorkspaceResolver, bool) {
	n, ok := r.Node.(BatchSpecWorkspaceResolver)
	if !ok {
		return nil, ok
	}
	return n.ToVisibleBatchSpecWorkspace()
}

func (r *NodeResolver) ToInsightsDashboard() (InsightsDashboardResolver, bool) {
	n, ok := r.Node.(InsightsDashboardResolver)
	return n, ok
}

func (r *NodeResolver) ToInsightView() (InsightViewResolver, bool) {
	n, ok := r.Node.(InsightViewResolver)
	return n, ok
}

func (r *NodeResolver) ToWebhookLog() (*webhookLogResolver, bool) {
	n, ok := r.Node.(*webhookLogResolver)
	return n, ok
}

func (r *NodeResolver) ToExecutor() (*executor.ExecutorResolver, bool) {
	n, ok := r.Node.(*executor.ExecutorResolver)
	return n, ok
}

func (r *NodeResolver) ToLockfileIndex() (LockfileIndexResolver, bool) {
	n, ok := r.Node.(LockfileIndexResolver)
	return n, ok
}

func (r *NodeResolver) ToExternalServiceSyncJob() (*externalServiceSyncJobResolver, bool) {
	n, ok := r.Node.(*externalServiceSyncJobResolver)
	return n, ok
}
