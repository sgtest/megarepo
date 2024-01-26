package api

import (
	"net/http"
	"strconv"

	"github.com/grafana/grafana/pkg/api/response"
	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/middleware"
	"github.com/grafana/grafana/pkg/middleware/requestmeta"
	ac "github.com/grafana/grafana/pkg/services/accesscontrol"
	contextmodel "github.com/grafana/grafana/pkg/services/contexthandler/model"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
)

func NewAccessControlAPI(router routing.RouteRegister, accesscontrol ac.AccessControl, service ac.Service,
	features featuremgmt.FeatureToggles) *AccessControlAPI {
	return &AccessControlAPI{
		RouteRegister: router,
		Service:       service,
		AccessControl: accesscontrol,
		features:      features,
	}
}

type AccessControlAPI struct {
	Service       ac.Service
	AccessControl ac.AccessControl
	RouteRegister routing.RouteRegister
	features      featuremgmt.FeatureToggles
}

func (api *AccessControlAPI) RegisterAPIEndpoints() {
	authorize := ac.Middleware(api.AccessControl)
	// Users
	api.RouteRegister.Group("/api/access-control", func(rr routing.RouteRegister) {
		rr.Get("/user/actions", middleware.ReqSignedIn, routing.Wrap(api.getUserActions))
		rr.Get("/user/permissions", middleware.ReqSignedIn, routing.Wrap(api.getUserPermissions))
		if api.features.IsEnabledGlobally(featuremgmt.FlagAccessControlOnCall) {
			rr.Get("/users/permissions/search", authorize(ac.EvalPermission(ac.ActionUsersPermissionsRead)), routing.Wrap(api.searchUsersPermissions))
		}
	}, requestmeta.SetOwner(requestmeta.TeamAuth))
}

// GET /api/access-control/user/actions
func (api *AccessControlAPI) getUserActions(c *contextmodel.ReqContext) response.Response {
	reloadCache := c.QueryBool("reloadcache")
	permissions, err := api.Service.GetUserPermissions(c.Req.Context(),
		c.SignedInUser, ac.Options{ReloadCache: reloadCache})
	if err != nil {
		return response.JSON(http.StatusInternalServerError, err)
	}

	return response.JSON(http.StatusOK, ac.BuildPermissionsMap(permissions))
}

// GET /api/access-control/user/permissions
func (api *AccessControlAPI) getUserPermissions(c *contextmodel.ReqContext) response.Response {
	reloadCache := c.QueryBool("reloadcache")
	permissions, err := api.Service.GetUserPermissions(c.Req.Context(),
		c.SignedInUser, ac.Options{ReloadCache: reloadCache})
	if err != nil {
		return response.JSON(http.StatusInternalServerError, err)
	}

	return response.JSON(http.StatusOK, ac.GroupScopesByAction(permissions))
}

// GET /api/access-control/users/permissions/search
func (api *AccessControlAPI) searchUsersPermissions(c *contextmodel.ReqContext) response.Response {
	searchOptions := ac.SearchOptions{
		UserLogin:    c.Query("userLogin"),
		ActionPrefix: c.Query("actionPrefix"),
		Action:       c.Query("action"),
		Scope:        c.Query("scope"),
	}

	userIDString := c.Query("userId")
	if userIDString != "" {
		userID, err := strconv.ParseInt(userIDString, 10, 64)
		if err != nil {
			return response.Error(http.StatusBadRequest, "user ID is invalid", err)
		}
		searchOptions.UserID = userID
	}

	// Validate inputs
	if (searchOptions.ActionPrefix != "") && (searchOptions.Action != "") {
		return response.JSON(http.StatusBadRequest, "'action' and 'actionPrefix' are mutually exclusive")
	}
	if (searchOptions.UserLogin != "") && (searchOptions.UserID > 0) {
		return response.JSON(http.StatusBadRequest, "'userId' and 'userLogin' are mutually exclusive")
	}
	if searchOptions.UserID <= 0 && searchOptions.UserLogin == "" &&
		searchOptions.ActionPrefix == "" && searchOptions.Action == "" {
		return response.JSON(http.StatusBadRequest, "at least one search option must be provided")
	}

	// Compute metadata
	permissions, err := api.Service.SearchUsersPermissions(c.Req.Context(), c.SignedInUser, searchOptions)
	if err != nil {
		return response.Error(http.StatusInternalServerError, "could not get org user permissions", err)
	}

	permsByAction := map[int64]map[string][]string{}
	for userID, userPerms := range permissions {
		permsByAction[userID] = ac.Reduce(userPerms)
	}

	return response.JSON(http.StatusOK, permsByAction)
}
