package resolvers

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	ee "github.com/sourcegraph/sourcegraph/enterprise/pkg/a8n"
	"github.com/sourcegraph/sourcegraph/internal/a8n"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// Resolver is the GraphQL resolver of all things A8N.
type Resolver struct {
	store       *ee.Store
	httpFactory *httpcli.Factory
}

// NewResolver returns a new Resolver whose store uses the given db
func NewResolver(db *sql.DB) graphqlbackend.A8NResolver {
	return &Resolver{store: ee.NewStore(db)}
}

func (r *Resolver) ChangesetByID(ctx context.Context, id graphql.ID) (graphqlbackend.ExternalChangesetResolver, error) {
	// 🚨 SECURITY: Only site admins may access changesets for now.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	changesetID, err := unmarshalChangesetID(id)
	if err != nil {
		return nil, err
	}

	changeset, err := r.store.GetChangeset(ctx, ee.GetChangesetOpts{ID: changesetID})
	if err != nil {
		return nil, err
	}

	return &changesetResolver{store: r.store, Changeset: changeset}, nil
}

func (r *Resolver) CampaignByID(ctx context.Context, id graphql.ID) (graphqlbackend.CampaignResolver, error) {
	// 🚨 SECURITY: Only site admins may access campaigns for now.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	campaignID, err := unmarshalCampaignID(id)
	if err != nil {
		return nil, err
	}

	campaign, err := r.store.GetCampaign(ctx, ee.GetCampaignOpts{ID: campaignID})
	if err != nil {
		return nil, err
	}

	return &campaignResolver{store: r.store, Campaign: campaign}, nil
}

func (r *Resolver) CampaignPlanByID(ctx context.Context, id graphql.ID) (graphqlbackend.CampaignPlanResolver, error) {
	// 🚨 SECURITY: Only site admins may access campaign plans for now.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	planID, err := unmarshalCampaignPlanID(id)
	if err != nil {
		return nil, err
	}

	plan, err := r.store.GetCampaignPlan(ctx, ee.GetCampaignPlanOpts{ID: planID})
	if err != nil {
		return nil, err
	}

	return &campaignPlanResolver{store: r.store, campaignPlan: plan}, nil
}

func (r *Resolver) AddChangesetsToCampaign(ctx context.Context, args *graphqlbackend.AddChangesetsToCampaignArgs) (_ graphqlbackend.CampaignResolver, err error) {
	// 🚨 SECURITY: Only site admins may modify changesets and campaigns for now.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	campaignID, err := unmarshalCampaignID(args.Campaign)
	if err != nil {
		return nil, err
	}

	changesetIDs := make([]int64, 0, len(args.Changesets))
	set := map[int64]struct{}{}
	for _, changesetID := range args.Changesets {
		id, err := unmarshalChangesetID(changesetID)
		if err != nil {
			return nil, err
		}

		if _, ok := set[id]; !ok {
			changesetIDs = append(changesetIDs, id)
			set[id] = struct{}{}
		}
	}

	tx, err := r.store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	defer tx.Done(&err)

	campaign, err := tx.GetCampaign(ctx, ee.GetCampaignOpts{ID: campaignID})
	if err != nil {
		return nil, err
	}
	// TODO(a8n): If campaign.CampaignPlainID != 0, we need to return an error
	// here. Only "manual" campaigns can have changesets added.

	changesets, _, err := tx.ListChangesets(ctx, ee.ListChangesetsOpts{IDs: changesetIDs})
	if err != nil {
		return nil, err
	}

	for _, c := range changesets {
		delete(set, c.ID)
		c.CampaignIDs = append(c.CampaignIDs, campaign.ID)
	}

	if len(set) > 0 {
		return nil, errors.Errorf("changesets %v not found", set)
	}

	if err = tx.UpdateChangesets(ctx, changesets...); err != nil {
		return nil, err
	}

	campaign.ChangesetIDs = append(campaign.ChangesetIDs, changesetIDs...)
	if err = tx.UpdateCampaign(ctx, campaign); err != nil {
		return nil, err
	}

	return &campaignResolver{store: r.store, Campaign: campaign}, nil
}

func (r *Resolver) CreateCampaign(ctx context.Context, args *graphqlbackend.CreateCampaignArgs) (graphqlbackend.CampaignResolver, error) {
	user, err := db.Users.GetByCurrentAuthUser(ctx)
	if err != nil {
		return nil, errors.Wrapf(err, "%v", backend.ErrNotAuthenticated)
	}

	// 🚨 SECURITY: Only site admins may create a campaign for now.
	if !user.SiteAdmin {
		return nil, backend.ErrMustBeSiteAdmin
	}

	campaign := &a8n.Campaign{
		Name:        args.Input.Name,
		Description: args.Input.Description,
		AuthorID:    user.ID,
	}

	if args.Input.Plan != nil {
		planID, err := unmarshalCampaignPlanID(*args.Input.Plan)
		if err != nil {
			return nil, err
		}
		campaign.CampaignPlanID = planID
		// TODO(a8n): Implement this. We need to turn the CampaignJobs into
		// changesets on the code host.
	}

	switch relay.UnmarshalKind(args.Input.Namespace) {
	case "User":
		relay.UnmarshalSpec(args.Input.Namespace, &campaign.NamespaceUserID)
	case "Org":
		relay.UnmarshalSpec(args.Input.Namespace, &campaign.NamespaceOrgID)
	default:
		return nil, errors.Errorf("Invalid namespace %q", args.Input.Namespace)
	}

	if err := r.store.CreateCampaign(ctx, campaign); err != nil {
		return nil, err
	}

	return &campaignResolver{store: r.store, Campaign: campaign}, nil
}

func (r *Resolver) UpdateCampaign(ctx context.Context, args *graphqlbackend.UpdateCampaignArgs) (graphqlbackend.CampaignResolver, error) {
	// 🚨 SECURITY: Only site admins may update campaigns for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	campaignID, err := unmarshalCampaignID(args.Input.ID)
	if err != nil {
		return nil, err
	}

	tx, err := r.store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	defer tx.Done(&err)

	campaign, err := tx.GetCampaign(ctx, ee.GetCampaignOpts{ID: campaignID})
	if err != nil {
		return nil, err
	}

	if args.Input.Name != nil {
		campaign.Name = *args.Input.Name
	}

	if args.Input.Description != nil {
		campaign.Description = *args.Input.Description
	}

	if err := tx.UpdateCampaign(ctx, campaign); err != nil {
		return nil, err
	}

	return &campaignResolver{store: r.store, Campaign: campaign}, nil
}

func (r *Resolver) DeleteCampaign(ctx context.Context, args *graphqlbackend.DeleteCampaignArgs) (*graphqlbackend.EmptyResponse, error) {
	// 🚨 SECURITY: Only site admins may update campaigns for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	campaignID, err := unmarshalCampaignID(args.Campaign)
	if err != nil {
		return nil, err
	}

	err = r.store.DeleteCampaign(ctx, campaignID)
	if err != nil {
		return nil, err
	}

	return &graphqlbackend.EmptyResponse{}, nil
}

func (r *Resolver) RetryCampaign(ctx context.Context, args *graphqlbackend.RetryCampaignArgs) (graphqlbackend.CampaignResolver, error) {
	// 🚨 SECURITY: Only site admins may update campaigns for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	campaignID, err := unmarshalCampaignID(args.Campaign)
	if err != nil {
		return nil, err
	}

	campaign, err := r.store.GetCampaign(ctx, ee.GetCampaignOpts{ID: campaignID})
	if err != nil {
		return nil, err
	}

	// TODO(a8n): Implement the retrying of turning "diffs" into changesets

	return &campaignResolver{store: r.store, Campaign: campaign}, nil
}

func (r *Resolver) Campaigns(ctx context.Context, args *graphqlutil.ConnectionArgs) (graphqlbackend.CampaignsConnectionResolver, error) {
	// 🚨 SECURITY: Only site admins may read campaigns for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	return &campaignsConnectionResolver{
		store: r.store,
		opts: ee.ListCampaignsOpts{
			Limit: int(args.GetFirst()),
		},
	}, nil
}

func (r *Resolver) CreateChangesets(ctx context.Context, args *graphqlbackend.CreateChangesetsArgs) (_ []graphqlbackend.ExternalChangesetResolver, err error) {
	// 🚨 SECURITY: Only site admins may create changesets for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	var repoIDs []uint32
	repoSet := map[uint32]*repos.Repo{}
	cs := make([]*a8n.Changeset, 0, len(args.Input))

	for _, c := range args.Input {
		repoID, err := unmarshalRepositoryID(c.Repository)
		if err != nil {
			return nil, err
		}

		id := uint32(repoID)
		if _, ok := repoSet[id]; !ok {
			repoSet[id] = nil
			repoIDs = append(repoIDs, id)
		}

		cs = append(cs, &a8n.Changeset{
			RepoID:     int32(id),
			ExternalID: c.ExternalID,
		})
	}

	tx, err := r.store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	defer tx.Done(&err)

	store := repos.NewDBStore(tx.DB(), sql.TxOptions{})

	rs, err := store.ListRepos(ctx, repos.StoreListReposArgs{IDs: repoIDs})
	if err != nil {
		return nil, err
	}

	for _, r := range rs {
		repoSet[r.ID] = r
	}

	for id, r := range repoSet {
		if r == nil {
			return nil, errors.Errorf("repo %v not found", marshalRepositoryID(api.RepoID(id)))
		}
	}

	for _, c := range cs {
		c.ExternalServiceType = repoSet[uint32(c.RepoID)].ExternalRepo.ServiceType
	}

	err = tx.CreateChangesets(ctx, cs...)
	if err != nil {
		if _, ok := err.(ee.AlreadyExistError); !ok {
			return nil, err
		}
	}

	tx.Done()

	// Only fetch metadata if none of these changesets existed before.
	// We do this outside of a transaction.

	store = repos.NewDBStore(r.store.DB(), sql.TxOptions{})
	syncer := ee.ChangesetSyncer{
		ReposStore:  store,
		Store:       r.store,
		HTTPFactory: r.httpFactory,
	}
	if err = syncer.SyncChangesets(ctx, cs...); err != nil {
		return nil, err
	}

	csr := make([]graphqlbackend.ExternalChangesetResolver, len(cs))
	for i := range cs {
		csr[i] = &changesetResolver{
			store:     r.store,
			Changeset: cs[i],
			repo:      repoSet[uint32(cs[i].RepoID)],
		}
	}

	return csr, nil
}

func (r *Resolver) Changesets(ctx context.Context, args *graphqlutil.ConnectionArgs) (graphqlbackend.ExternalChangesetsConnectionResolver, error) {
	// 🚨 SECURITY: Only site admins may read changesets for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	return &changesetsConnectionResolver{
		store: r.store,
		opts: ee.ListChangesetsOpts{
			Limit: int(args.GetFirst()),
		},
	}, nil
}

func (r *Resolver) PreviewCampaignPlan(ctx context.Context, args graphqlbackend.PreviewCampaignPlanArgs) (graphqlbackend.CampaignPlanResolver, error) {
	// 🚨 SECURITY: Only site admins may update campaigns for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	typeName := strings.ToLower(args.Specification.Type)
	if typeName == "" {
		return nil, errors.New("cannot create CampaignPlan without Type")
	}
	campaignType, ok := a8n.CampaignTypes[typeName]
	if !ok {
		return nil, errors.New("campaign type does not exist. Don't know how to plan this campaign")
	}

	specArgs := make(map[string]string)
	// TODO(a8n): This turns the JSONC into JSON. We need to preserve the JSONC.
	if err := jsonc.Unmarshal(string(args.Specification.Arguments), &specArgs); err != nil {
		return nil, err
	}
	if len(specArgs) != len(campaignType.Parameters) {
		return nil, errors.New("not enough arguments for campaign type specified")
	}
	for _, param := range campaignType.Parameters {
		if _, ok := specArgs[param]; !ok {
			return nil, fmt.Errorf("missing argument in specification: %s", param)
		}
	}

	plan := &a8n.CampaignPlan{
		CampaignType: typeName,
		Arguments:    string(args.Specification.Arguments),
	}
	if err := r.store.CreateCampaignPlan(ctx, plan); err != nil {
		return nil, err
	}

	// Search repositories over which to execute code modification
	repos, err := graphqlbackend.SearchRepos(ctx, specArgs["searchScope"])
	if err != nil {
		return nil, err
	}

	var wg sync.WaitGroup
	for _, repo := range repos {
		job := &a8n.CampaignJob{
			CampaignPlanID: plan.ID,
			StartedAt:      time.Now().UTC(),
		}

		err := relay.UnmarshalSpec(repo.ID(), &job.RepoID)
		if err != nil {
			return nil, err
		}

		defaultBranch, err := repo.DefaultBranch(ctx)
		if err != nil {
			return nil, err
		}
		if defaultBranch != nil {
			commit, err := defaultBranch.Target().Commit(ctx)
			if err != nil {
				return nil, err
			}
			job.Rev = api.CommitID(commit.OID())
		}

		err = r.store.CreateCampaignJob(ctx, job)
		if err != nil {
			return nil, err
		}

		wg.Add(1)
		go func(plan *a8n.CampaignPlan, job *a8n.CampaignJob) {
			// TODO(a8n): Do real work.
			// Send request to service with Repo, Ref, Arguments.
			// Receive diff.
			job.Diff = bogusDiff
			job.FinishedAt = time.Now()

			err := r.store.UpdateCampaignJob(ctx, job)
			if err != nil {
				log15.Error("RunCampaign.UpdateCampaignJob failed", "err", err)
			}

			wg.Done()
		}(plan, job)
	}

	// TODO(a8n): Implement this so that `if !args.Wait` this mutation is
	// asynchronous
	wg.Wait()

	return &campaignPlanResolver{store: r.store, campaignPlan: plan}, nil
}

const bogusDiff = `diff --git a/README.md b/README.md
index 323fae0..34a3ec2 100644
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-foobar
+barfoo
`

func (r *Resolver) CancelCampaignPlan(ctx context.Context, args graphqlbackend.CancelCampaignPlanArgs) (*graphqlbackend.EmptyResponse, error) {
	// 🚨 SECURITY: Only site admins may update campaigns for now
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	id, err := unmarshalCampaignPlanID(args.Plan)
	if err != nil {
		return nil, err
	}

	_, err = r.store.GetCampaignPlan(ctx, ee.GetCampaignPlanOpts{ID: id})
	if err != nil {
		return nil, err
	}

	// TODO(a8n): Implement this. We need to cancel plan so that all jobs are stopped.
	return &graphqlbackend.EmptyResponse{}, nil
}
