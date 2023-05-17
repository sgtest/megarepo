package graphqlbackend

import (
	"context"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
)

type ListOwnershipArgs struct {
	First   *int32
	After   *string
	Reasons *[]string
}

type OwnResolver interface {
	GitBlobOwnership(ctx context.Context, blob *GitTreeEntryResolver, args ListOwnershipArgs) (OwnershipConnectionResolver, error)
	GitCommitOwnership(ctx context.Context, commit *GitCommitResolver, args ListOwnershipArgs) (OwnershipConnectionResolver, error)
	GitTreeOwnership(ctx context.Context, tree *GitTreeEntryResolver, args ListOwnershipArgs) (OwnershipConnectionResolver, error)

	PersonOwnerField(person *PersonResolver) string
	UserOwnerField(user *UserResolver) string
	TeamOwnerField(team *TeamResolver) string

	NodeResolvers() map[string]NodeByIDFunc

	// Codeowners queries
	CodeownersIngestedFiles(context.Context, *CodeownersIngestedFilesArgs) (CodeownersIngestedFileConnectionResolver, error)
	RepoIngestedCodeowners(context.Context, api.RepoID) (CodeownersIngestedFileResolver, error)

	// Codeowners mutations
	AddCodeownersFile(context.Context, *CodeownersFileArgs) (CodeownersIngestedFileResolver, error)
	UpdateCodeownersFile(context.Context, *CodeownersFileArgs) (CodeownersIngestedFileResolver, error)
	DeleteCodeownersFiles(context.Context, *DeleteCodeownersFileArgs) (*EmptyResponse, error)

	// config
	OwnSignalConfigurations(ctx context.Context) ([]SignalConfigurationResolver, error)
	UpdateOwnSignalConfigurations(ctx context.Context, configurationsArgs UpdateSignalConfigurationsArgs) ([]SignalConfigurationResolver, error)
}

type OwnershipConnectionResolver interface {
	TotalCount(context.Context) (int32, error)
	TotalOwners(context.Context) (int32, error)
	PageInfo(context.Context) (*graphqlutil.PageInfo, error)
	Nodes(context.Context) ([]OwnershipResolver, error)
}

type Ownable interface {
	ToGitBlob(context.Context) (*GitTreeEntryResolver, bool)
}

type OwnershipResolver interface {
	Owner(context.Context) (OwnerResolver, error)
	Reasons(context.Context) ([]OwnershipReasonResolver, error)
}

type OwnerResolver interface {
	OwnerField(context.Context) (string, error)

	ToPerson() (*PersonResolver, bool)
	ToTeam() (*TeamResolver, bool)
}

type OwnershipReasonResolver interface {
	SimpleOwnReasonResolver
	ToCodeownersFileEntry() (CodeownersFileEntryResolver, bool)
	ToRecentContributorOwnershipSignal() (RecentContributorOwnershipSignalResolver, bool)
	ToRecentViewOwnershipSignal() (RecentViewOwnershipSignalResolver, bool)
}

type SimpleOwnReasonResolver interface {
	Title() (string, error)
	Description() (string, error)
}

type CodeownersFileEntryResolver interface {
	Title() (string, error)
	Description() (string, error)
	CodeownersFile(context.Context) (FileResolver, error)
	RuleLineMatch(context.Context) (int32, error)
}

type RecentContributorOwnershipSignalResolver interface {
	Title() (string, error)
	Description() (string, error)
}

type RecentViewOwnershipSignalResolver interface {
	Title() (string, error)
	Description() (string, error)
}

type CodeownersFileArgs struct {
	Input CodeownersFileInput
}

type CodeownersFileInput struct {
	FileContents string
	RepoID       *graphql.ID
	RepoName     *string
}

type DeleteCodeownersFilesInput struct {
	RepoID   *graphql.ID
	RepoName *string
}

type DeleteCodeownersFileArgs struct {
	Repositories []DeleteCodeownersFilesInput
}

type CodeownersIngestedFilesArgs struct {
	First *int32
	After *string
}

type CodeownersIngestedFileResolver interface {
	ID() graphql.ID
	Contents() string
	Repository() *RepositoryResolver
	CreatedAt() gqlutil.DateTime
	UpdatedAt() gqlutil.DateTime
}

type CodeownersIngestedFileConnectionResolver interface {
	Nodes(ctx context.Context) ([]CodeownersIngestedFileResolver, error)
	TotalCount(ctx context.Context) (int32, error)
	PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error)
}

type SignalConfigurationResolver interface {
	Name() string
	Description() string
	IsEnabled() bool
	ExcludedRepoPatterns() []string
}

type UpdateSignalConfigurationsArgs struct {
	Input UpdateSignalConfigurationsInput
}

type UpdateSignalConfigurationsInput struct {
	Configs []SignalConfigurationUpdate
}

type SignalConfigurationUpdate struct {
	Name                 string
	ExcludedRepoPatterns []string
	Enabled              bool
}
