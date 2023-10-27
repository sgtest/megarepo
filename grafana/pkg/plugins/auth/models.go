package auth

import (
	"context"

	"github.com/grafana/grafana/pkg/plugins/plugindef"
)

type ExternalService struct {
	ClientID     string `json:"clientId"`
	ClientSecret string `json:"clientSecret"`
	PrivateKey   string `json:"privateKey"`
}

type ExternalServiceRegistry interface {
	RegisterExternalService(ctx context.Context, name string, pType plugindef.Type, svc *plugindef.ExternalServiceRegistration) (*ExternalService, error)
}
