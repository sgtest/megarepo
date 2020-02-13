package graphqlbackend

import (
	"context"
	"database/sql"
	"errors"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/externallink"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/a8n"
)

// NewA8NResolver will be set by enterprise
var NewA8NResolver func(*sql.DB) A8NResolver

type AddChangesetsToCampaignArgs struct {
	Campaign   graphql.ID
	Changesets []graphql.ID
}

type CreateCampaignArgs struct {
	Input struct {
		Namespace   graphql.ID
		Name        string
		Description string
		Branch      *string
		Plan        *graphql.ID
		Draft       *bool
	}
}

type UpdateCampaignArgs struct {
	Input struct {
		ID          graphql.ID
		Name        *string
		Description *string
		Branch      *string
		Plan        *graphql.ID
	}
}

type CreateCampaignPlanFromPatchesArgs struct {
	Patches []CampaignPlanPatch
}

type CampaignPlanPatch struct {
	Repository   graphql.ID
	BaseRevision string
	Patch        string
}

type ListCampaignArgs struct {
	First *int32
	State *string
}

type DeleteCampaignArgs struct {
	Campaign        graphql.ID
	CloseChangesets bool
}

type RetryCampaignArgs struct {
	Campaign graphql.ID
}

type CloseCampaignArgs struct {
	Campaign        graphql.ID
	CloseChangesets bool
}

type CreateChangesetsArgs struct {
	Input []struct {
		Repository graphql.ID
		ExternalID string
	}
}

type PublishCampaignArgs struct {
	Campaign graphql.ID
}

type PublishChangesetArgs struct {
	ChangesetPlan graphql.ID
}

type A8NResolver interface {
	CreateCampaign(ctx context.Context, args *CreateCampaignArgs) (CampaignResolver, error)
	UpdateCampaign(ctx context.Context, args *UpdateCampaignArgs) (CampaignResolver, error)
	CampaignByID(ctx context.Context, id graphql.ID) (CampaignResolver, error)
	Campaigns(ctx context.Context, args *ListCampaignArgs) (CampaignsConnectionResolver, error)
	DeleteCampaign(ctx context.Context, args *DeleteCampaignArgs) (*EmptyResponse, error)
	RetryCampaign(ctx context.Context, args *RetryCampaignArgs) (CampaignResolver, error)
	CloseCampaign(ctx context.Context, args *CloseCampaignArgs) (CampaignResolver, error)
	PublishCampaign(ctx context.Context, args *PublishCampaignArgs) (CampaignResolver, error)
	PublishChangeset(ctx context.Context, args *PublishChangesetArgs) (*EmptyResponse, error)

	CreateChangesets(ctx context.Context, args *CreateChangesetsArgs) ([]ExternalChangesetResolver, error)
	ChangesetByID(ctx context.Context, id graphql.ID) (ExternalChangesetResolver, error)
	Changesets(ctx context.Context, args *graphqlutil.ConnectionArgs) (ExternalChangesetsConnectionResolver, error)

	AddChangesetsToCampaign(ctx context.Context, args *AddChangesetsToCampaignArgs) (CampaignResolver, error)

	CreateCampaignPlanFromPatches(ctx context.Context, args CreateCampaignPlanFromPatchesArgs) (CampaignPlanResolver, error)
	CampaignPlanByID(ctx context.Context, id graphql.ID) (CampaignPlanResolver, error)

	ChangesetPlanByID(ctx context.Context, id graphql.ID) (ChangesetPlanResolver, error)
}

var a8nOnlyInEnterprise = errors.New("campaigns and changesets are only available in enterprise")

type defaultA8NResolver struct{}

func (defaultA8NResolver) CreateCampaign(ctx context.Context, args *CreateCampaignArgs) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) UpdateCampaign(ctx context.Context, args *UpdateCampaignArgs) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) CampaignByID(ctx context.Context, id graphql.ID) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) Campaigns(ctx context.Context, args *ListCampaignArgs) (CampaignsConnectionResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) DeleteCampaign(ctx context.Context, args *DeleteCampaignArgs) (*EmptyResponse, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) RetryCampaign(ctx context.Context, args *RetryCampaignArgs) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) CloseCampaign(ctx context.Context, args *CloseCampaignArgs) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) PublishCampaign(ctx context.Context, args *PublishCampaignArgs) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) PublishChangeset(ctx context.Context, args *PublishChangesetArgs) (*EmptyResponse, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) CreateChangesets(ctx context.Context, args *CreateChangesetsArgs) ([]ExternalChangesetResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) ChangesetByID(ctx context.Context, id graphql.ID) (ExternalChangesetResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) Changesets(ctx context.Context, args *graphqlutil.ConnectionArgs) (ExternalChangesetsConnectionResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) AddChangesetsToCampaign(ctx context.Context, args *AddChangesetsToCampaignArgs) (CampaignResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) CreateCampaignPlanFromPatches(ctx context.Context, args CreateCampaignPlanFromPatchesArgs) (CampaignPlanResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) CampaignPlanByID(ctx context.Context, id graphql.ID) (CampaignPlanResolver, error) {
	return nil, a8nOnlyInEnterprise
}

func (defaultA8NResolver) ChangesetPlanByID(ctx context.Context, id graphql.ID) (ChangesetPlanResolver, error) {
	return nil, a8nOnlyInEnterprise
}

type ChangesetCountsArgs struct {
	From *DateTime
	To   *DateTime
}

type CampaignResolver interface {
	ID() graphql.ID
	Name() string
	Description() string
	Branch() *string
	Author(ctx context.Context) (*UserResolver, error)
	ViewerCanAdminister(ctx context.Context) (bool, error)
	URL(ctx context.Context) (string, error)
	Namespace(ctx context.Context) (n NamespaceResolver, err error)
	CreatedAt() DateTime
	UpdatedAt() DateTime
	Changesets(ctx context.Context, args struct{ graphqlutil.ConnectionArgs }) ExternalChangesetsConnectionResolver
	ChangesetCountsOverTime(ctx context.Context, args *ChangesetCountsArgs) ([]ChangesetCountsResolver, error)
	RepositoryDiffs(ctx context.Context, args *graphqlutil.ConnectionArgs) (RepositoryComparisonConnectionResolver, error)
	Plan(ctx context.Context) (CampaignPlanResolver, error)
	Status(context.Context) (BackgroundProcessStatus, error)
	ClosedAt() *DateTime
	PublishedAt(ctx context.Context) (*DateTime, error)
	ChangesetPlans(ctx context.Context, args *graphqlutil.ConnectionArgs) ChangesetPlansConnectionResolver
}

type CampaignsConnectionResolver interface {
	Nodes(ctx context.Context) ([]CampaignResolver, error)
	TotalCount(ctx context.Context) (int32, error)
	PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error)
}

type ExternalChangesetsConnectionResolver interface {
	Nodes(ctx context.Context) ([]ExternalChangesetResolver, error)
	TotalCount(ctx context.Context) (int32, error)
	PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error)
}

type ChangesetLabelResolver interface {
	Text() string
	Color() string
	Description() *string
}

type ExternalChangesetResolver interface {
	ID() graphql.ID
	ExternalID() string
	CreatedAt() DateTime
	UpdatedAt() DateTime
	Title() (string, error)
	Body() (string, error)
	State() (a8n.ChangesetState, error)
	ExternalURL() (*externallink.Resolver, error)
	ReviewState(context.Context) (a8n.ChangesetReviewState, error)
	CheckState(context.Context) (*a8n.ChangesetCheckState, error)
	Repository(ctx context.Context) (*RepositoryResolver, error)
	Campaigns(ctx context.Context, args *ListCampaignArgs) (CampaignsConnectionResolver, error)
	Events(ctx context.Context, args *struct{ graphqlutil.ConnectionArgs }) (ChangesetEventsConnectionResolver, error)
	Diff(ctx context.Context) (*RepositoryComparisonResolver, error)
	Head(ctx context.Context) (*GitRefResolver, error)
	Base(ctx context.Context) (*GitRefResolver, error)
	Labels(ctx context.Context) ([]ChangesetLabelResolver, error)
}

type ChangesetPlansConnectionResolver interface {
	Nodes(ctx context.Context) ([]ChangesetPlanResolver, error)
	TotalCount(ctx context.Context) (int32, error)
	PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error)
}

type ChangesetPlanResolver interface {
	ID() graphql.ID
	Repository(ctx context.Context) (*RepositoryResolver, error)
	BaseRepository(ctx context.Context) (*RepositoryResolver, error)
	Diff() ChangesetPlanResolver
	FileDiffs(ctx context.Context, args *graphqlutil.ConnectionArgs) (PreviewFileDiffConnection, error)
	PublicationEnqueued(ctx context.Context) (bool, error)
}

type ChangesetEventsConnectionResolver interface {
	Nodes(ctx context.Context) ([]ChangesetEventResolver, error)
	TotalCount(ctx context.Context) (int32, error)
	PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error)
}

type ChangesetEventResolver interface {
	ID() graphql.ID
	Changeset(ctx context.Context) (ExternalChangesetResolver, error)
	CreatedAt() DateTime
}

type ChangesetCountsResolver interface {
	Date() DateTime
	Total() int32
	Merged() int32
	Closed() int32
	Open() int32
	OpenApproved() int32
	OpenChangesRequested() int32
	OpenPending() int32
}

type CampaignPlanArgResolver interface {
	Name() string
	Value() string
}

type BackgroundProcessStatus interface {
	CompletedCount() int32
	PendingCount() int32

	State() a8n.BackgroundProcessState

	Errors() []string
}

type CampaignPlanResolver interface {
	ID() graphql.ID

	Status(ctx context.Context) (BackgroundProcessStatus, error)

	// DEPRECATED: Remove in 3.15 in favor of ChangesetPlans.
	Changesets(ctx context.Context, args *graphqlutil.ConnectionArgs) ChangesetPlansConnectionResolver

	ChangesetPlans(ctx context.Context, args *graphqlutil.ConnectionArgs) ChangesetPlansConnectionResolver

	PreviewURL() string
}

type PreviewFileDiff interface {
	OldPath() *string
	NewPath() *string
	Hunks() []*DiffHunk
	Stat() *DiffStat
	OldFile() *GitTreeEntryResolver
	InternalID() string
}

type PreviewFileDiffConnection interface {
	Nodes(ctx context.Context) ([]PreviewFileDiff, error)
	TotalCount(ctx context.Context) (*int32, error)
	PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error)
	DiffStat(ctx context.Context) (*DiffStat, error)
	RawDiff(ctx context.Context) (string, error)
}
