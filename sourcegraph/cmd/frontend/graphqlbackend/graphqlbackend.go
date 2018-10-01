package graphqlbackend

import (
	"context"
	"errors"
	"strconv"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	gqlerrors "github.com/graph-gophers/graphql-go/errors"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/graph-gophers/graphql-go/trace"
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
)

// GraphQLSchema is the parsed Schema with the root resolver attached. It is
// exported since it is accessed in our httpapi.
var GraphQLSchema *graphql.Schema

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

func init() {
	var err error
	GraphQLSchema, err = graphql.ParseSchema(Schema, &schemaResolver{}, graphql.Tracer(prometheusTracer{}))
	if err != nil {
		panic(err)
	}
}

// EmptyResponse is a type that can be used in the return signature for graphql queries
// that don't require a return value.
type EmptyResponse struct{}

// AlwaysNil exists since various graphql tools expect at least one field to be
// present in the schema so we provide a dummy one here that is always nil.
func (er *EmptyResponse) AlwaysNil() *string {
	return nil
}

type node interface {
	ID() graphql.ID
}

type nodeResolver struct {
	node
}

func (r *nodeResolver) ToAccessToken() (*accessTokenResolver, bool) {
	n, ok := r.node.(*accessTokenResolver)
	return n, ok
}

func (r *nodeResolver) ToDependency() (*dependencyResolver, bool) {
	n, ok := r.node.(*dependencyResolver)
	return n, ok
}

func (r *nodeResolver) ToProductLicense() (ProductLicense, bool) {
	n, ok := r.node.(ProductLicense)
	return n, ok
}

func (r *nodeResolver) ToProductSubscription() (ProductSubscription, bool) {
	n, ok := r.node.(ProductSubscription)
	return n, ok
}

func (r *nodeResolver) ToExternalAccount() (*externalAccountResolver, bool) {
	n, ok := r.node.(*externalAccountResolver)
	return n, ok
}

func (r *nodeResolver) ToGitRef() (*gitRefResolver, bool) {
	n, ok := r.node.(*gitRefResolver)
	return n, ok
}

func (r *nodeResolver) ToRepository() (*repositoryResolver, bool) {
	n, ok := r.node.(*repositoryResolver)
	return n, ok
}

func (r *nodeResolver) ToUser() (*UserResolver, bool) {
	n, ok := r.node.(*UserResolver)
	return n, ok
}

func (r *nodeResolver) ToOrg() (*OrgResolver, bool) {
	n, ok := r.node.(*OrgResolver)
	return n, ok
}

func (r *nodeResolver) ToOrganizationInvitation() (*organizationInvitationResolver, bool) {
	n, ok := r.node.(*organizationInvitationResolver)
	return n, ok
}

func (r *nodeResolver) ToGitCommit() (*gitCommitResolver, bool) {
	n, ok := r.node.(*gitCommitResolver)
	return n, ok
}

func (r *nodeResolver) ToPackage() (*packageResolver, bool) {
	n, ok := r.node.(*packageResolver)
	return n, ok
}

func (r *nodeResolver) ToRegistryExtension() (RegistryExtension, bool) {
	return NodeToRegistryExtension(r.node)
}

func (r *nodeResolver) ToSite() (*siteResolver, bool) {
	n, ok := r.node.(*siteResolver)
	return n, ok
}

type schemaResolver struct{}

// DEPRECATED
func (r *schemaResolver) Root() *schemaResolver {
	return &schemaResolver{}
}

func (r *schemaResolver) Node(ctx context.Context, args *struct{ ID graphql.ID }) (*nodeResolver, error) {
	n, err := nodeByID(ctx, args.ID)
	if err != nil {
		return nil, err
	}
	return &nodeResolver{n}, nil
}

func nodeByID(ctx context.Context, id graphql.ID) (node, error) {
	switch relay.UnmarshalKind(id) {
	case "AccessToken":
		return accessTokenByID(ctx, id)
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
	case "GitRef":
		return gitRefByID(ctx, id)
	case "Dependency":
		return dependencyByID(ctx, id)
	case "Repository":
		return repositoryByID(ctx, id)
	case "User":
		return UserByID(ctx, id)
	case "Org":
		return orgByID(ctx, id)
	case "OrganizationInvitation":
		return orgInvitationByID(ctx, id)
	case "GitCommit":
		return gitCommitByID(ctx, id)
	case "Package":
		return packageByID(ctx, id)
	case "RegistryExtension":
		return RegistryExtensionByID(ctx, id)
	case "SavedQuery":
		return savedQueryByID(ctx, id)
	case "Site":
		return siteByGQLID(ctx, id)
	default:
		return nil, errors.New("invalid id")
	}
}

func (r *schemaResolver) Repository(ctx context.Context, args *struct {
	Name *string
	// TODO(chris): Remove URI in favor of Name.
	URI *string
}) (*repositoryResolver, error) {
	if args.Name != nil {
		args.URI = args.Name
	}

	if args.URI == nil {
		return nil, nil
	}

	repo, err := backend.Repos.GetByURI(ctx, api.RepoURI(*args.URI))
	if err != nil {
		if err, ok := err.(backend.ErrRepoSeeOther); ok {
			return &repositoryResolver{repo: &types.Repo{}, redirectURL: &err.RedirectURL}, nil
		}
		if errcode.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}

	if err := refreshRepo(ctx, repo); err != nil {
		return nil, err
	}

	return &repositoryResolver{repo: repo}, nil
}

func (r *schemaResolver) PhabricatorRepo(ctx context.Context, args *struct {
	Name *string
	// TODO(chris): Remove URI in favor of Name.
	URI *string
}) (*phabricatorRepoResolver, error) {
	if args.Name != nil {
		args.URI = args.Name
	}

	repo, err := db.Phabricator.GetByURI(ctx, api.RepoURI(*args.URI))
	if err != nil {
		return nil, err
	}
	return &phabricatorRepoResolver{repo}, nil
}

var skipRefresh = false // set by tests

func refreshRepo(ctx context.Context, repo *types.Repo) error {
	if skipRefresh {
		return nil
	}
	return backend.Repos.RefreshIndex(ctx, repo)
}

func (r *schemaResolver) CurrentUser(ctx context.Context) (*UserResolver, error) {
	return CurrentUser(ctx)
}
