package graphqlbackend

import (
	"context"
	"strings"
	"sync"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

type externalServiceResolver struct {
	logger          log.Logger
	db              database.DB
	externalService *types.ExternalService
	warning         string

	webhookURLOnce sync.Once
	webhookURL     string
	webhookErr     error
}

const externalServiceIDKind = "ExternalService"

func externalServiceByID(ctx context.Context, db database.DB, gqlID graphql.ID) (*externalServiceResolver, error) {
	id, err := UnmarshalExternalServiceID(gqlID)

	if err != nil {
		return nil, err
	}

	es, err := db.ExternalServices().GetByID(ctx, id)
	if err != nil {
		return nil, err
	}

	if err := backend.CheckExternalServiceAccess(ctx, db, es.NamespaceUserID, es.NamespaceOrgID); err != nil {
		return nil, err
	}
	return &externalServiceResolver{logger: log.Scoped("externalServiceResolver", ""), db: db, externalService: es}, nil
}

func MarshalExternalServiceID(id int64) graphql.ID {
	return relay.MarshalID(externalServiceIDKind, id)
}

func UnmarshalExternalServiceID(id graphql.ID) (externalServiceID int64, err error) {
	if kind := relay.UnmarshalKind(id); kind != externalServiceIDKind {
		err = errors.Errorf("expected graphql ID to have kind %q; got %q", externalServiceIDKind, kind)
		return
	}
	err = relay.UnmarshalSpec(id, &externalServiceID)
	return
}

func (r *externalServiceResolver) ID() graphql.ID {
	return MarshalExternalServiceID(r.externalService.ID)
}

func (r *externalServiceResolver) Kind() string {
	return r.externalService.Kind
}

func (r *externalServiceResolver) DisplayName() string {
	return r.externalService.DisplayName
}

func (r *externalServiceResolver) Config() (JSONCString, error) {
	redacted, err := r.externalService.RedactedConfig()
	if err != nil {
		return "", err
	}
	return JSONCString(redacted), nil
}

func (r *externalServiceResolver) CreatedAt() DateTime {
	return DateTime{Time: r.externalService.CreatedAt}
}

func (r *externalServiceResolver) UpdatedAt() DateTime {
	return DateTime{Time: r.externalService.UpdatedAt}
}

func (r *externalServiceResolver) Namespace(ctx context.Context) (*NamespaceResolver, error) {
	if r.externalService.NamespaceUserID == 0 {
		return nil, nil
	}
	userID := MarshalUserID(r.externalService.NamespaceUserID)
	n, err := NamespaceByID(ctx, r.db, userID)
	if err != nil {
		return nil, err
	}
	return &NamespaceResolver{n}, nil
}

func (r *externalServiceResolver) WebhookURL() (*string, error) {
	r.webhookURLOnce.Do(func() {
		parsed, err := extsvc.ParseConfig(r.externalService.Kind, r.externalService.Config)
		if err != nil {
			r.webhookErr = errors.Wrap(err, "parsing external service config")
			return
		}
		u, err := extsvc.WebhookURL(r.externalService.Kind, r.externalService.ID, parsed, conf.ExternalURL())
		if err != nil {
			r.webhookErr = errors.Wrap(err, "building webhook URL")
		}
		// If no webhook URL can be built for the kind, we bail out and don't throw an error.
		if u == "" {
			return
		}
		switch c := parsed.(type) {
		case *schema.BitbucketCloudConnection:
			if c.WebhookSecret != "" {
				r.webhookURL = u
			}
		case *schema.BitbucketServerConnection:
			if c.Webhooks != nil {
				r.webhookURL = u
			}
			if c.Plugin != nil && c.Plugin.Webhooks != nil {
				r.webhookURL = u
			}
		case *schema.GitHubConnection:
			if len(c.Webhooks) > 0 {
				r.webhookURL = u
			}
		case *schema.GitLabConnection:
			if len(c.Webhooks) > 0 {
				r.webhookURL = u
			}
		}
	})
	if r.webhookURL == "" {
		return nil, r.webhookErr
	}
	return &r.webhookURL, r.webhookErr
}

func (r *externalServiceResolver) Warning() *string {
	if r.warning == "" {
		return nil
	}
	return &r.warning
}

func (r *externalServiceResolver) LastSyncError(ctx context.Context) (*string, error) {
	latestError, err := r.db.ExternalServices().GetLastSyncError(ctx, r.externalService.ID)
	if err != nil {
		return nil, err
	}
	if latestError == "" {
		return nil, nil
	}
	return &latestError, nil
}

func (r *externalServiceResolver) RepoCount(ctx context.Context) (int32, error) {
	return r.db.ExternalServices().RepoCount(ctx, r.externalService.ID)
}

func (r *externalServiceResolver) LastSyncAt() *DateTime {
	if r.externalService.LastSyncAt.IsZero() {
		return nil
	}
	return &DateTime{Time: r.externalService.LastSyncAt}
}

func (r *externalServiceResolver) NextSyncAt() *DateTime {
	if r.externalService.NextSyncAt.IsZero() {
		return nil
	}
	return &DateTime{Time: r.externalService.NextSyncAt}
}

var scopeCache = rcache.New("extsvc_token_scope")

func (r *externalServiceResolver) GrantedScopes(ctx context.Context) (*[]string, error) {
	scopes, err := repos.GrantedScopes(ctx, r.logger.Scoped("GrantedScopes", ""), scopeCache, r.db, r.externalService)
	if err != nil {
		// It's possible that we fail to fetch scope from the code host, in this case we
		// don't want the entire resolver to fail.
		log15.Error("Getting service scope", "id", r.externalService.ID, "error", err)
		return nil, nil
	}
	if scopes == nil {
		return nil, nil
	}
	return &scopes, nil
}

func (r *externalServiceResolver) WebhookLogs(ctx context.Context, args *webhookLogsArgs) (*webhookLogConnectionResolver, error) {
	return newWebhookLogConnectionResolver(ctx, r.db, args, webhookLogsExternalServiceID(r.externalService.ID))
}

type externalServiceSyncJobsArgs struct {
	First *int32
}

func (r *externalServiceResolver) SyncJobs(ctx context.Context, args *externalServiceSyncJobsArgs) (*externalServiceSyncJobConnectionResolver, error) {
	return newExternalServiceSyncJobConnectionResolver(ctx, r.db, args, r.externalService.ID)
}

type externalServiceSyncJobConnectionResolver struct {
	logger            log.Logger
	args              *externalServiceSyncJobsArgs
	externalServiceID int64
	db                database.DB

	once       sync.Once
	nodes      []*types.ExternalServiceSyncJob
	totalCount int64
	err        error
}

func newExternalServiceSyncJobConnectionResolver(ctx context.Context, db database.DB, args *externalServiceSyncJobsArgs, externalServiceID int64) (*externalServiceSyncJobConnectionResolver, error) {
	return &externalServiceSyncJobConnectionResolver{
		args:              args,
		externalServiceID: externalServiceID,
		db:                db,
	}, nil
}

func (r *externalServiceSyncJobConnectionResolver) Nodes(ctx context.Context) ([]*externalServiceSyncJobResolver, error) {
	jobs, _, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}

	nodes := make([]*externalServiceSyncJobResolver, len(jobs))
	for i, j := range jobs {
		nodes[i] = &externalServiceSyncJobResolver{
			job: j,
		}
	}

	return nodes, nil
}

func (r *externalServiceSyncJobConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	_, totalCount, err := r.compute(ctx)
	return int32(totalCount), err
}

func (r *externalServiceSyncJobConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	jobs, totalCount, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	return graphqlutil.HasNextPage(len(jobs) != int(totalCount)), nil
}

func (r *externalServiceSyncJobConnectionResolver) compute(ctx context.Context) ([]*types.ExternalServiceSyncJob, int64, error) {
	r.once.Do(func() {
		opts := database.ExternalServicesGetSyncJobsOptions{
			ExternalServiceID: r.externalServiceID,
		}
		if r.args.First != nil {
			opts.LimitOffset = &database.LimitOffset{
				Limit: int(*r.args.First),
			}
		}
		r.nodes, r.err = r.db.ExternalServices().GetSyncJobs(ctx, opts)
		if r.err != nil {
			return
		}
		r.totalCount, r.err = r.db.ExternalServices().CountSyncJobs(ctx, opts)
	})

	return r.nodes, r.totalCount, r.err
}

type externalServiceSyncJobResolver struct {
	job *types.ExternalServiceSyncJob
}

func marshalExternalServiceSyncJobID(id int64) graphql.ID {
	return relay.MarshalID("ExternalServiceSyncJob", id)
}

func unmarshalExternalServiceSyncJobID(id graphql.ID) (jobID int64, err error) {
	err = relay.UnmarshalSpec(id, &jobID)
	return
}

func externalServiceSyncJobByID(ctx context.Context, db database.DB, gqlID graphql.ID) (Node, error) {
	// Site-admin only for now.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx, db); err != nil {
		return nil, err
	}

	id, err := unmarshalExternalServiceSyncJobID(gqlID)
	if err != nil {
		return nil, err
	}

	job, err := db.ExternalServices().GetSyncJobByID(ctx, id)
	if err != nil {
		if errcode.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}

	return &externalServiceSyncJobResolver{job: job}, nil
}

func (r *externalServiceSyncJobResolver) ID() graphql.ID {
	return marshalExternalServiceSyncJobID(r.job.ID)
}

func (r *externalServiceSyncJobResolver) State() string {
	return strings.ToUpper(r.job.State)
}

func (r *externalServiceSyncJobResolver) FailureMessage() *string {
	if r.job.FailureMessage == "" {
		return nil
	}

	return &r.job.FailureMessage
}

func (r *externalServiceSyncJobResolver) QueuedAt() DateTime {
	return DateTime{Time: r.job.QueuedAt}
}

func (r *externalServiceSyncJobResolver) StartedAt() *DateTime {
	if r.job.StartedAt.IsZero() {
		return nil
	}

	return &DateTime{Time: r.job.StartedAt}
}

func (r *externalServiceSyncJobResolver) FinishedAt() *DateTime {
	if r.job.FinishedAt.IsZero() {
		return nil
	}

	return &DateTime{Time: r.job.FinishedAt}
}
