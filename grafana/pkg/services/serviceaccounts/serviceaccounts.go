package serviceaccounts

import (
	"context"

	"github.com/grafana/grafana/pkg/services/apikey"
)

/*
ServiceAccountService is the service that manages service accounts.

Service accounts are used to authenticate API requests. They are not users and
do not have a password.
*/
type Service interface {
	CreateServiceAccount(ctx context.Context, orgID int64, saForm *CreateServiceAccountForm) (*ServiceAccountDTO, error)
	DeleteServiceAccount(ctx context.Context, orgID, serviceAccountID int64) error
	RetrieveServiceAccount(ctx context.Context, orgID, serviceAccountID int64) (*ServiceAccountProfileDTO, error)
	RetrieveServiceAccountIdByName(ctx context.Context, orgID int64, name string) (int64, error)
	UpdateServiceAccount(ctx context.Context, orgID, serviceAccountID int64,
		saForm *UpdateServiceAccountForm) (*ServiceAccountProfileDTO, error)
	AddServiceAccountToken(ctx context.Context, serviceAccountID int64,
		cmd *AddServiceAccountTokenCommand) (*apikey.APIKey, error)
}

//go:generate mockery --name ExtSvcAccountsService --structname MockExtSvcAccountsService --output tests --outpkg tests --filename extsvcaccmock.go
type ExtSvcAccountsService interface {
	// ManageExtSvcAccount creates, updates or deletes the service account associated with an external service
	ManageExtSvcAccount(ctx context.Context, cmd *ManageExtSvcAccountCmd) (int64, error)
	// RetrieveExtSvcAccount fetches an external service account by ID
	RetrieveExtSvcAccount(ctx context.Context, orgID, saID int64) (*ExtSvcAccount, error)
}
