package graphqlbackend

import (
	"context"
	"errors"
	"strconv"
	"time"

	"github.com/graph-gophers/graphql-go"
	gqlerrors "github.com/graph-gophers/graphql-go/errors"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/graph-gophers/graphql-go/trace"
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
)

var graphqlFieldHistogram = prometheus.NewHistogramVec(prometheus.HistogramOpts{
	Namespace: "src",
	Subsystem: "graphql",
	Name:      "field_seconds",
	Help:      "GraphQL field resolver latencies in seconds.",
	Buckets:   []float64{0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 30},
}, []string{"type", "field", "error"})

func init() {
	prometheus.MustRegister(graphqlFieldHistogram)
}

type prometheusTracer struct {
	trace.OpenTracingTracer
}

func (prometheusTracer) TraceField(ctx context.Context, label, typeName, fieldName string, trivial bool, args map[string]interface{}) (context.Context, trace.TraceFieldFinishFunc) {
	traceCtx, finish := trace.OpenTracingTracer{}.TraceField(ctx, label, typeName, fieldName, trivial, args)
	start := time.Now()
	return traceCtx, func(err *gqlerrors.QueryError) {
		graphqlFieldHistogram.WithLabelValues(typeName, fieldName, strconv.FormatBool(err != nil)).Observe(time.Since(start).Seconds())
		finish(err)
	}
}

func NewSchema(a8n A8NResolver) (*graphql.Schema, error) {
	return graphql.ParseSchema(
		Schema,
		&schemaResolver{a8nResolver: a8n},
		graphql.Tracer(prometheusTracer{}),
	)
}

// EmptyResponse is a type that can be used in the return signature for graphql queries
// that don't require a return value.
type EmptyResponse struct{}

// AlwaysNil exists since various graphql tools expect at least one field to be
// present in the schema so we provide a dummy one here that is always nil.
func (er *EmptyResponse) AlwaysNil() *string {
	return nil
}

type Node interface {
	ID() graphql.ID
}

type NodeResolver struct {
	Node
}

func (r *NodeResolver) ToAccessToken() (*accessTokenResolver, bool) {
	n, ok := r.Node.(*accessTokenResolver)
	return n, ok
}

func (r *NodeResolver) ToCampaign() (CampaignResolver, bool) {
	n, ok := r.Node.(CampaignResolver)
	return n, ok
}

func (r *NodeResolver) ToCampaignPlan() (CampaignPlanResolver, bool) {
	n, ok := r.Node.(CampaignPlanResolver)
	return n, ok
}

func (r *NodeResolver) ToChangeset() (ChangesetResolver, bool) {
	n, ok := r.Node.(ChangesetResolver)
	return n, ok
}

func (r *NodeResolver) ToExternalChangeset() (ExternalChangesetResolver, bool) {
	n, ok := r.Node.(ExternalChangesetResolver)
	return n, ok
}

func (r *NodeResolver) ToChangesetEvent() (ChangesetEventResolver, bool) {
	n, ok := r.Node.(ChangesetEventResolver)
	return n, ok
}

func (r *NodeResolver) ToDiscussionComment() (*discussionCommentResolver, bool) {
	n, ok := r.Node.(*discussionCommentResolver)
	return n, ok
}

func (r *NodeResolver) ToDiscussionThread() (*discussionThreadResolver, bool) {
	n, ok := r.Node.(*discussionThreadResolver)
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

func (r *NodeResolver) ToSite() (*siteResolver, bool) {
	n, ok := r.Node.(*siteResolver)
	return n, ok
}

// schemaResolver handles all GraphQL queries for Sourcegraph.  To do this, it
// uses subresolvers, some of which are globals and some of which are fields on
// schemaResolver.
type schemaResolver struct {
	a8nResolver A8NResolver
}

// DEPRECATED
func (r *schemaResolver) Root() *schemaResolver {
	return &schemaResolver{}
}

func (r *schemaResolver) Node(ctx context.Context, args *struct{ ID graphql.ID }) (*NodeResolver, error) {
	n, err := r.nodeByID(ctx, args.ID)
	if err != nil {
		return nil, err
	}
	return &NodeResolver{n}, nil
}

func (r *schemaResolver) nodeByID(ctx context.Context, id graphql.ID) (Node, error) {
	switch relay.UnmarshalKind(id) {
	case "AccessToken":
		return accessTokenByID(ctx, id)
	case "Campaign":
		if r.a8nResolver == nil {
			return nil, onlyInEnterprise
		}
		return r.a8nResolver.CampaignByID(ctx, id)
	case "CampaignPlan":
		if r.a8nResolver == nil {
			return nil, onlyInEnterprise
		}
		return r.a8nResolver.CampaignPlanByID(ctx, id)
	case "ExternalChangeset":
		if r.a8nResolver == nil {
			return nil, onlyInEnterprise
		}
		return r.a8nResolver.ChangesetByID(ctx, id)
	case "DiscussionComment":
		return discussionCommentByID(ctx, id)
	case "DiscussionThread":
		return discussionThreadByID(ctx, id)
	case "ProductLicense":
		if f := ProductLicenseByID; f != nil {
			return f(ctx, id)
		}
		return nil, errors.New("not implemented")
	case "ProductSubscription":
		if f := ProductSubscriptionByID; f != nil {
			return f(ctx, id)
		}
		return nil, errors.New("not implemented")
	case "ExternalAccount":
		return externalAccountByID(ctx, id)
	case externalServiceIDKind:
		return externalServiceByID(ctx, id)
	case "GitRef":
		return gitRefByID(ctx, id)
	case "Repository":
		return repositoryByID(ctx, id)
	case "User":
		return UserByID(ctx, id)
	case "Org":
		return OrgByID(ctx, id)
	case "OrganizationInvitation":
		return orgInvitationByID(ctx, id)
	case "GitCommit":
		return gitCommitByID(ctx, id)
	case "RegistryExtension":
		return RegistryExtensionByID(ctx, id)
	case "SavedSearch":
		return savedSearchByID(ctx, id)
	case "Site":
		return siteByGQLID(ctx, id)
	default:
		return nil, errors.New("invalid id")
	}
}

func (r *schemaResolver) Repository(ctx context.Context, args *struct {
	Name     *string
	CloneURL *string
	// TODO(chris): Remove URI in favor of Name.
	URI *string
}) (*RepositoryResolver, error) {
	var name api.RepoName
	if args.URI != nil {
		// Deprecated query by "URI"
		name = api.RepoName(*args.URI)
	} else if args.Name != nil {
		// Query by name
		name = api.RepoName(*args.Name)
	} else if args.CloneURL != nil {
		// Query by git clone URL
		var err error
		name, err = reposourceCloneURLToRepoName(ctx, *args.CloneURL)
		if err != nil {
			return nil, err
		}
		if name == "" {
			// Clone URL could not be mapped to a code host
			return nil, nil
		}
	} else {
		return nil, errors.New("Neither name nor cloneURL given")
	}

	repo, err := backend.Repos.GetByName(ctx, name)
	if err != nil {
		if err, ok := err.(backend.ErrRepoSeeOther); ok {
			return &RepositoryResolver{repo: &types.Repo{}, redirectURL: &err.RedirectURL}, nil
		}
		if errcode.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}
	return &RepositoryResolver{repo: repo}, nil
}

func (r *schemaResolver) PhabricatorRepo(ctx context.Context, args *struct {
	Name *string
	// TODO(chris): Remove URI in favor of Name.
	URI *string
}) (*phabricatorRepoResolver, error) {
	if args.Name != nil {
		args.URI = args.Name
	}

	repo, err := db.Phabricator.GetByName(ctx, api.RepoName(*args.URI))
	if err != nil {
		return nil, err
	}
	return &phabricatorRepoResolver{repo}, nil
}

func (r *schemaResolver) CurrentUser(ctx context.Context) (*UserResolver, error) {
	return CurrentUser(ctx)
}
