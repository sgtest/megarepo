package graphqlbackend

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graph-gophers/graphql-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var extsvcConfigAllowEdits, _ = strconv.ParseBool(env.Get("EXTSVC_CONFIG_ALLOW_EDITS", "false", "When EXTSVC_CONFIG_FILE is in use, allow edits in the application to be made which will be overwritten on next process restart"))

var extsvcConfigFile = env.Get("EXTSVC_CONFIG_FILE", "", "EXTSVC_CONFIG_FILE can contain configurations for multiple code host connections. See https://docs.sourcegraph.com/admin/config/advanced_config_file for details.")

func externalServicesWritable() error {
	if extsvcConfigFile != "" && !extsvcConfigAllowEdits {
		return errors.New("adding external service not allowed when using EXTSVC_CONFIG_FILE")
	}
	return nil
}

const syncExternalServiceTimeout = 15 * time.Second

type addExternalServiceArgs struct {
	Input addExternalServiceInput
}

type addExternalServiceInput struct {
	Kind        string
	DisplayName string
	Config      string
	Namespace   *graphql.ID
}

func (r *schemaResolver) AddExternalService(ctx context.Context, args *addExternalServiceArgs) (*externalServiceResolver, error) {
	start := time.Now()
	// 🚨 SECURITY: Only site admins may add external services. User's external services are not supported anymore.
	var err error
	defer reportExternalServiceDuration(start, Add, &err)

	if err := externalServicesWritable(); err != nil {
		return nil, err
	}

	if auth.CheckCurrentUserIsSiteAdmin(ctx, r.db) != nil {
		err = auth.ErrMustBeSiteAdmin
		return nil, err
	}

	externalService := &types.ExternalService{
		Kind:        args.Input.Kind,
		DisplayName: args.Input.DisplayName,
		Config:      extsvc.NewUnencryptedConfig(args.Input.Config),
	}

	if err = r.db.ExternalServices().Create(ctx, conf.Get, externalService); err != nil {
		return nil, err
	}

	res := &externalServiceResolver{logger: r.logger.Scoped("externalServiceResolver", ""), db: r.db, externalService: externalService}
	if err = backend.SyncExternalService(ctx, r.logger, externalService, syncExternalServiceTimeout, r.repoupdaterClient); err != nil {
		res.warning = fmt.Sprintf("External service created, but we encountered a problem while validating the external service: %s", err)
	}

	return res, nil
}

type updateExternalServiceArgs struct {
	Input updateExternalServiceInput
}

type updateExternalServiceInput struct {
	ID          graphql.ID
	DisplayName *string
	Config      *string
}

func (r *schemaResolver) UpdateExternalService(ctx context.Context, args *updateExternalServiceArgs) (*externalServiceResolver, error) {
	start := time.Now()
	var err error
	defer reportExternalServiceDuration(start, Update, &err)

	if err := externalServicesWritable(); err != nil {
		return nil, err
	}

	// 🚨 SECURITY: check whether user is site-admin
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	id, err := UnmarshalExternalServiceID(args.Input.ID)
	if err != nil {
		return nil, err
	}

	es, err := r.db.ExternalServices().GetByID(ctx, id)
	if err != nil {
		return nil, err
	}

	oldConfig, err := es.Config.Decrypt(ctx)
	if err != nil {
		return nil, err
	}

	if args.Input.Config != nil && strings.TrimSpace(*args.Input.Config) == "" {
		err = errors.New("blank external service configuration is invalid (must be valid JSONC)")
		return nil, err
	}

	ps := conf.Get().AuthProviders
	update := &database.ExternalServiceUpdate{
		DisplayName: args.Input.DisplayName,
		Config:      args.Input.Config,
	}
	if err = r.db.ExternalServices().Update(ctx, ps, id, update); err != nil {
		return nil, err
	}

	// Fetch from database again to get all fields with updated values.
	es, err = r.db.ExternalServices().GetByID(ctx, id)
	if err != nil {
		return nil, err
	}
	newConfig, err := es.Config.Decrypt(ctx)
	if err != nil {
		return nil, err
	}

	res := &externalServiceResolver{logger: r.logger.Scoped("externalServiceResolver", ""), db: r.db, externalService: es}

	if oldConfig != newConfig {
		err = backend.SyncExternalService(ctx, r.logger, es, syncExternalServiceTimeout, r.repoupdaterClient)
		if err != nil {
			res.warning = fmt.Sprintf("External service updated, but we encountered a problem while validating the external service: %s", err)
		}
	}

	return res, nil
}

type deleteExternalServiceArgs struct {
	ExternalService graphql.ID
	Async           bool
}

func (r *schemaResolver) DeleteExternalService(ctx context.Context, args *deleteExternalServiceArgs) (*EmptyResponse, error) {
	start := time.Now()
	var err error
	defer reportExternalServiceDuration(start, Delete, &err)

	if err := externalServicesWritable(); err != nil {
		return nil, err
	}

	// 🚨 SECURITY: check whether user is site-admin
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	id, err := UnmarshalExternalServiceID(args.ExternalService)
	if err != nil {
		return nil, err
	}

	// Load external service to make sure it exists
	_, err = r.db.ExternalServices().GetByID(ctx, id)
	if err != nil {
		return nil, err
	}

	if args.Async {
		// run deletion in the background and return right away
		go func() {
			if err := r.db.ExternalServices().Delete(context.Background(), id); err != nil {
				r.logger.Error("Background external service deletion failed", log.Error(err))
			}
		}()
	} else {
		if err := r.db.ExternalServices().Delete(ctx, id); err != nil {
			return nil, err
		}
	}

	return &EmptyResponse{}, nil
}

type ExternalServicesArgs struct {
	graphqlutil.ConnectionArgs
	After     *string
	Namespace *graphql.ID
}

func (r *schemaResolver) ExternalServices(ctx context.Context, args *ExternalServicesArgs) (*externalServiceConnectionResolver, error) {
	// 🚨 SECURITY: check whether user is site-admin
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	var namespaceUserID int32
	var namespaceOrgID int32

	var afterID int64
	if args.After != nil {
		var err error
		afterID, err = UnmarshalExternalServiceID(graphql.ID(*args.After))
		if err != nil {
			return nil, err
		}
	}

	opt := database.ExternalServicesListOptions{
		// 🚨 SECURITY: When both `namespaceUserID` and `namespaceOrgID` are not
		// specified we need to explicitly specify `NoNamespace`, otherwise site
		// admins will be able to list all user code host connections that are not
		// accessible when trying to access them individually.
		NoNamespace:     namespaceUserID == 0 && namespaceOrgID == 0,
		NamespaceUserID: namespaceUserID,
		NamespaceOrgID:  namespaceOrgID,
		AfterID:         afterID,
	}
	args.ConnectionArgs.Set(&opt.LimitOffset)
	return &externalServiceConnectionResolver{db: r.db, opt: opt}, nil
}

type externalServiceConnectionResolver struct {
	opt database.ExternalServicesListOptions

	// cache results because they are used by multiple fields
	once             sync.Once
	externalServices []*types.ExternalService
	err              error
	db               database.DB
}

func (r *externalServiceConnectionResolver) compute(ctx context.Context) ([]*types.ExternalService, error) {
	r.once.Do(func() {
		r.externalServices, r.err = r.db.ExternalServices().List(ctx, r.opt)
	})
	return r.externalServices, r.err
}

func (r *externalServiceConnectionResolver) Nodes(ctx context.Context) ([]*externalServiceResolver, error) {
	externalServices, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	resolvers := make([]*externalServiceResolver, 0, len(externalServices))
	for _, externalService := range externalServices {
		resolvers = append(resolvers, &externalServiceResolver{logger: log.Scoped("externalServiceResolver", ""), db: r.db, externalService: externalService})
	}
	return resolvers, nil
}

func (r *externalServiceConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	// Reset pagination cursor to get correct total count
	opt := r.opt
	opt.AfterID = 0
	count, err := r.db.ExternalServices().Count(ctx, opt)
	return int32(count), err
}

func (r *externalServiceConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	externalServices, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}

	// We would have had all results when no limit set
	if r.opt.LimitOffset == nil {
		return graphqlutil.HasNextPage(false), nil
	}

	// We got less results than limit, means we've had all results
	if len(externalServices) < r.opt.Limit {
		return graphqlutil.HasNextPage(false), nil
	}

	// In case the number of results happens to be the same as the limit,
	// we need another query to get accurate total count with same cursor
	// to determine if there are more results than the limit we set.
	count, err := r.db.ExternalServices().Count(ctx, r.opt)
	if err != nil {
		return nil, err
	}

	if count > len(externalServices) {
		endCursorID := externalServices[len(externalServices)-1].ID
		return graphqlutil.NextPageCursor(string(MarshalExternalServiceID(endCursorID))), nil
	}
	return graphqlutil.HasNextPage(false), nil
}

type computedExternalServiceConnectionResolver struct {
	args             graphqlutil.ConnectionArgs
	externalServices []*types.ExternalService
	db               database.DB
}

func (r *computedExternalServiceConnectionResolver) Nodes(ctx context.Context) []*externalServiceResolver {
	svcs := r.externalServices
	if r.args.First != nil && int(*r.args.First) < len(svcs) {
		svcs = svcs[:*r.args.First]
	}
	resolvers := make([]*externalServiceResolver, 0, len(svcs))
	for _, svc := range svcs {
		resolvers = append(resolvers, &externalServiceResolver{logger: log.Scoped("externalServiceResolver", ""), db: r.db, externalService: svc})
	}
	return resolvers
}

func (r *computedExternalServiceConnectionResolver) TotalCount(ctx context.Context) int32 {
	return int32(len(r.externalServices))
}

func (r *computedExternalServiceConnectionResolver) PageInfo(ctx context.Context) *graphqlutil.PageInfo {
	return graphqlutil.HasNextPage(r.args.First != nil && len(r.externalServices) >= int(*r.args.First))
}

type ExternalServiceMutationType int

const (
	Add ExternalServiceMutationType = iota
	Update
	Delete
	SetRepos
)

func (d ExternalServiceMutationType) String() string {
	return []string{"add", "update", "delete", "set-repos"}[d]
}

var mutationDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
	Name:    "src_extsvc_mutation_duration_seconds",
	Help:    "ExternalService mutation latencies in seconds.",
	Buckets: trace.UserLatencyBuckets,
}, []string{"success", "mutation", "namespace"})

func reportExternalServiceDuration(startTime time.Time, mutation ExternalServiceMutationType, err *error) {
	duration := time.Since(startTime)
	ns := "global"
	labels := prometheus.Labels{
		"mutation":  mutation.String(),
		"success":   strconv.FormatBool(*err == nil),
		"namespace": ns,
	}
	mutationDuration.With(labels).Observe(duration.Seconds())
}

type syncExternalServiceArgs struct {
	ID graphql.ID
}

func (r *schemaResolver) SyncExternalService(ctx context.Context, args *syncExternalServiceArgs) (*EmptyResponse, error) {
	start := time.Now()
	var err error
	defer reportExternalServiceDuration(start, Update, &err)

	// 🚨 SECURITY: check whether user is site-admin
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	id, err := UnmarshalExternalServiceID(args.ID)
	if err != nil {
		return nil, err
	}

	es, err := r.db.ExternalServices().GetByID(ctx, id)
	if err != nil {
		return nil, err
	}

	// Enqueue a sync job for the external service, if none exists yet.
	rstore := repos.NewStore(r.logger, r.db)
	if err := rstore.EnqueueSingleSyncJob(ctx, es.ID); err != nil {
		return nil, err
	}

	return &EmptyResponse{}, nil
}

type cancelExternalServiceSyncArgs struct {
	ID graphql.ID
}

func (r *schemaResolver) CancelExternalServiceSync(ctx context.Context, args *cancelExternalServiceSyncArgs) (*EmptyResponse, error) {
	start := time.Now()
	var err error
	defer reportExternalServiceDuration(start, Update, &err)

	// 🚨 SECURITY: check whether user is site-admin
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	id, err := unmarshalExternalServiceSyncJobID(args.ID)
	if err != nil {
		return nil, err
	}

	esj, err := r.db.ExternalServices().GetSyncJobByID(ctx, id)
	if err != nil {
		return nil, err
	}
	_, err = r.db.ExternalServices().GetByID(ctx, esj.ExternalServiceID)
	if err != nil {
		return nil, err
	}

	if err := r.db.ExternalServices().CancelSyncJob(ctx, esj.ID); err != nil {
		return nil, err
	}

	return &EmptyResponse{}, nil
}
