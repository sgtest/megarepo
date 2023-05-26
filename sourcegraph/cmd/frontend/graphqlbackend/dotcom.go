package graphqlbackend

import (
	"context"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
)

type DotcomRootResolver interface {
	DotcomResolver
	Dotcom() DotcomResolver
	NodeResolvers() map[string]NodeByIDFunc
}

// DotcomResolver is the interface for the GraphQL types DotcomMutation and DotcomQuery.
type DotcomResolver interface {
	// DotcomMutation
	CreateProductSubscription(context.Context, *CreateProductSubscriptionArgs) (ProductSubscription, error)
	UpdateProductSubscription(context.Context, *UpdateProductSubscriptionArgs) (*EmptyResponse, error)
	GenerateProductLicenseForSubscription(context.Context, *GenerateProductLicenseForSubscriptionArgs) (ProductLicense, error)
	ArchiveProductSubscription(context.Context, *ArchiveProductSubscriptionArgs) (*EmptyResponse, error)

	// DotcomQuery
	ProductSubscription(context.Context, *ProductSubscriptionArgs) (ProductSubscription, error)
	ProductSubscriptions(context.Context, *ProductSubscriptionsArgs) (ProductSubscriptionConnection, error)
	ProductSubscriptionByAccessToken(context.Context, *ProductSubscriptionByAccessTokenArgs) (ProductSubscription, error)
	ProductLicenses(context.Context, *ProductLicensesArgs) (ProductLicenseConnection, error)
	ProductLicenseByID(ctx context.Context, id graphql.ID) (ProductLicense, error)
	ProductSubscriptionByID(ctx context.Context, id graphql.ID) (ProductSubscription, error)
}

// ProductSubscription is the interface for the GraphQL type ProductSubscription.
type ProductSubscription interface {
	ID() graphql.ID
	UUID() string
	Name() string
	Account(context.Context) (*UserResolver, error)
	ActiveLicense(context.Context) (ProductLicense, error)
	ProductLicenses(context.Context, *graphqlutil.ConnectionArgs) (ProductLicenseConnection, error)
	LLMProxyAccess() LLMProxyAccess
	CreatedAt() gqlutil.DateTime
	IsArchived() bool
	URL(context.Context) (string, error)
	URLForSiteAdmin(context.Context) *string
	CurrentSourcegraphAccessToken(context.Context) (*string, error)
	SourcegraphAccessTokens(context.Context) ([]string, error)
}

type CreateProductSubscriptionArgs struct {
	AccountID graphql.ID
}

type GenerateProductLicenseForSubscriptionArgs struct {
	ProductSubscriptionID graphql.ID
	License               *ProductLicenseInput
}

type GenerateAccessTokenForSubscriptionArgs struct {
	ProductSubscriptionID graphql.ID
}

// ProductSubscriptionAccessToken is the interface for the GraphQL type ProductSubscriptionAccessToken.
type ProductSubscriptionAccessToken interface {
	AccessToken() string
}

type ArchiveProductSubscriptionArgs struct{ ID graphql.ID }

type ProductSubscriptionArgs struct {
	UUID string
}

type ProductSubscriptionsArgs struct {
	graphqlutil.ConnectionArgs
	Account *graphql.ID
	Query   *string
}

// ProductSubscriptionConnection is the interface for the GraphQL type
// ProductSubscriptionConnection.
type ProductSubscriptionConnection interface {
	Nodes(context.Context) ([]ProductSubscription, error)
	TotalCount(context.Context) (int32, error)
	PageInfo(context.Context) (*graphqlutil.PageInfo, error)
}

// ProductLicense is the interface for the GraphQL type ProductLicense.
type ProductLicense interface {
	ID() graphql.ID
	Subscription(context.Context) (ProductSubscription, error)
	Info() (*ProductLicenseInfo, error)
	LicenseKey() string
	CreatedAt() gqlutil.DateTime
}

// ProductLicenseInput implements the GraphQL type ProductLicenseInput.
type ProductLicenseInput struct {
	Tags      []string
	UserCount int32
	ExpiresAt int32
}

type ProductLicensesArgs struct {
	graphqlutil.ConnectionArgs
	LicenseKeySubstring   *string
	ProductSubscriptionID *graphql.ID
}

// ProductLicenseConnection is the interface for the GraphQL type ProductLicenseConnection.
type ProductLicenseConnection interface {
	Nodes(context.Context) ([]ProductLicense, error)
	TotalCount(context.Context) (int32, error)
	PageInfo(context.Context) (*graphqlutil.PageInfo, error)
}

type ProductSubscriptionByAccessTokenArgs struct {
	AccessToken string
}

type UpdateProductSubscriptionArgs struct {
	ID     graphql.ID
	Update UpdateProductSubscriptionInput
}

type UpdateProductSubscriptionInput struct {
	LLMProxyAccess *UpdateLLMProxyAccessInput
}

type UpdateLLMProxyAccessInput struct {
	Enabled                                 *bool
	ChatCompletionsRateLimit                *int32
	ChatCompletionsRateLimitIntervalSeconds *int32
	ChatCompletionsAllowedModels            *[]string
	CodeCompletionsRateLimit                *int32
	CodeCompletionsRateLimitIntervalSeconds *int32
	CodeCompletionsAllowedModels            *[]string
}

type LLMProxyAccess interface {
	Enabled() bool
	ChatCompletionsRateLimit(context.Context) (LLMProxyRateLimit, error)
	CodeCompletionsRateLimit(context.Context) (LLMProxyRateLimit, error)
}

type LLMProxyUsageDatapoint interface {
	Date() gqlutil.DateTime
	Model() string
	Count() int32
}

type LLMProxyRateLimitSource string

const (
	LLMProxyRateLimitSourceOverride LLMProxyRateLimitSource = "OVERRIDE"
	LLMProxyRateLimitSourcePlan     LLMProxyRateLimitSource = "PLAN"
)

type LLMProxyRateLimit interface {
	Source() LLMProxyRateLimitSource
	AllowedModels() []string
	Limit() int32
	IntervalSeconds() int32
	Usage(context.Context) ([]LLMProxyUsageDatapoint, error)
}
