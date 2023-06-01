package graphqlbackend

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type externalAccountDataResolver struct {
	data *extsvc.PublicAccountData
}

func NewExternalAccountDataResolver(ctx context.Context, account extsvc.Account) (*externalAccountDataResolver, error) {
	data, err := publicAccountDataFromJSON(ctx, account)
	if err != nil || data == nil {
		return nil, err
	}
	return &externalAccountDataResolver{
		data: data,
	}, nil
}

func publicAccountDataFromJSON(ctx context.Context, account extsvc.Account) (*extsvc.PublicAccountData, error) {
	// each provider type implements the correct method ExternalAccountInfo, we do not
	// need a specific instance, just the first one of the same type
	p := providers.GetProviderbyServiceType(account.ServiceType)
	if p == nil {
		return nil, errors.Errorf("cannot find authorization provider for the external account, service type: %s", account.ServiceType)
	}

	return p.ExternalAccountInfo(ctx, account)
}

func (r *externalAccountDataResolver) DisplayName() *string {
	return r.data.DisplayName
}

func (r *externalAccountDataResolver) Login() *string {
	return r.data.Login
}

func (r *externalAccountDataResolver) URL() *string {
	return r.data.URL
}
