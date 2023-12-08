package api

import (
	"context"
	"errors"
	"net/http"

	"github.com/grafana/grafana/pkg/api/response"
	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/infra/log"
	ac "github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/auth/identity"
	contextmodel "github.com/grafana/grafana/pkg/services/contexthandler/model"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/ssosettings"
	"github.com/grafana/grafana/pkg/services/ssosettings/models"
	"github.com/grafana/grafana/pkg/web"
)

type Api struct {
	Log                log.Logger
	RouteRegister      routing.RouteRegister
	AccessControl      ac.AccessControl
	Features           *featuremgmt.FeatureManager
	SSOSettingsService ssosettings.Service
}

func ProvideApi(
	ssoSettingsSvc ssosettings.Service,
	routeRegister routing.RouteRegister,
	ac ac.AccessControl,
) *Api {
	api := &Api{
		SSOSettingsService: ssoSettingsSvc,
		RouteRegister:      routeRegister,
		AccessControl:      ac,
		Log:                log.New("ssosettings.api"),
	}

	return api
}

// RegisterAPIEndpoints Registers Endpoints on Grafana Router
func (api *Api) RegisterAPIEndpoints() {
	api.RouteRegister.Group("/api/v1/sso-settings", func(router routing.RouteRegister) {
		auth := ac.Middleware(api.AccessControl)

		scopeKey := ac.Parameter(":key")
		settingsScope := ac.ScopeSettingsOAuth(scopeKey)

		reqWriteAccess := auth(ac.EvalPermission(ac.ActionSettingsWrite, settingsScope))

		router.Get("/", auth(ac.EvalPermission(ac.ActionSettingsRead)), routing.Wrap(api.listAllProvidersSettings))
		router.Get("/:key", auth(ac.EvalPermission(ac.ActionSettingsRead, settingsScope)), routing.Wrap(api.getProviderSettings))
		router.Put("/:key", reqWriteAccess, routing.Wrap(api.updateProviderSettings))
		router.Delete("/:key", reqWriteAccess, routing.Wrap(api.removeProviderSettings))
	})
}

func (api *Api) listAllProvidersSettings(c *contextmodel.ReqContext) response.Response {
	providers, err := api.getAuthorizedList(c.Req.Context(), c.SignedInUser)
	if err != nil {
		return response.Error(500, "Failed to get providers", err)
	}

	dtos := make([]*models.SSOSettingsDTO, 0)
	for _, provider := range providers {
		dto, err := provider.ToSSOSettingsDTO()
		if err != nil {
			api.Log.Warn("Failed to convert SSO Settings for provider " + provider.Provider)
			continue
		}

		dtos = append(dtos, dto)
	}

	return response.JSON(http.StatusOK, dtos)
}

func (api *Api) getAuthorizedList(ctx context.Context, identity identity.Requester) ([]*models.SSOSettings, error) {
	allProviders, err := api.SSOSettingsService.List(ctx)
	if err != nil {
		return nil, err
	}

	var authorizedProviders []*models.SSOSettings
	for _, provider := range allProviders {
		ev := ac.EvalPermission(ac.ActionSettingsRead, ac.Scope("settings", "auth."+provider.Provider, "*"))
		hasAccess, err := api.AccessControl.Evaluate(ctx, identity, ev)
		if err != nil {
			api.Log.FromContext(ctx).Error("Failed to evaluate permissions", "error", err)
			return nil, err
		}

		if !hasAccess {
			continue
		}

		authorizedProviders = append(authorizedProviders, provider)
	}

	return authorizedProviders, nil
}

func (api *Api) getProviderSettings(c *contextmodel.ReqContext) response.Response {
	key, ok := web.Params(c.Req)[":key"]
	if !ok {
		return response.Error(http.StatusBadRequest, "Missing key", nil)
	}

	settings, err := api.SSOSettingsService.GetForProvider(c.Req.Context(), key)
	if err != nil {
		return response.Error(http.StatusNotFound, "The provider was not found", err)
	}

	dto, err := settings.ToSSOSettingsDTO()
	if err != nil {
		return response.Error(http.StatusInternalServerError, "The provider is invalid", err)
	}

	return response.JSON(http.StatusOK, dto)
}

func (api *Api) updateProviderSettings(c *contextmodel.ReqContext) response.Response {
	key, ok := web.Params(c.Req)[":key"]
	if !ok {
		return response.Error(http.StatusBadRequest, "Missing key", nil)
	}

	var settingsDTO models.SSOSettingsDTO
	if err := web.Bind(c.Req, &settingsDTO); err != nil {
		return response.Error(http.StatusBadRequest, "Failed to parse request body", err)
	}

	settings, err := settingsDTO.ToSSOSettings()
	if err != nil {
		return response.Error(http.StatusBadRequest, "Invalid request body", err)
	}

	settings.Provider = key

	err = api.SSOSettingsService.Upsert(c.Req.Context(), *settings)
	// TODO: first check whether the error is referring to validation errors

	// other error
	if err != nil {
		return response.Error(http.StatusInternalServerError, "Failed to update provider settings", err)
	}

	return response.JSON(http.StatusNoContent, nil)
}

// swagger:route DELETE /v1/sso-settings/{key} sso_settings removeProviderSettings
//
// # Remove SSO Settings
//
// # Remove an SSO Settings entry by Key
//
// You need to have a permission with action `settings:write` with scope `settings:auth.<provider>:*`.
//
// Responses:
// 204: okResponse
// 400: badRequestError
// 401: unauthorisedError
// 403: forbiddenError
// 500: internalServerError
func (api *Api) removeProviderSettings(c *contextmodel.ReqContext) response.Response {
	key, ok := web.Params(c.Req)[":key"]
	if !ok {
		return response.Error(http.StatusBadRequest, "Missing key", nil)
	}

	err := api.SSOSettingsService.Delete(c.Req.Context(), key)
	if err != nil {
		if errors.Is(err, ssosettings.ErrNotFound) {
			return response.Error(http.StatusNotFound, "The provider was not found", err)
		}
		return response.Error(http.StatusInternalServerError, "Failed to delete provider settings", err)
	}

	return response.JSON(http.StatusNoContent, nil)
}

// swagger:parameters removeProviderSettings
type RemoveProviderSettingsParams struct {
	// in:path
	// required:true
	Key string `json:"key"`
}
