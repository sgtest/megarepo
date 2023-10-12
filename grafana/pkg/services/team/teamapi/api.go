package teamapi

import (
	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/middleware/requestmeta"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/licensing"
	pref "github.com/grafana/grafana/pkg/services/preference"
	"github.com/grafana/grafana/pkg/services/team"
	"github.com/grafana/grafana/pkg/setting"
)

type TeamAPI struct {
	teamService            team.Service
	ac                     accesscontrol.Service
	teamPermissionsService accesscontrol.TeamPermissionsService
	license                licensing.Licensing
	cfg                    *setting.Cfg
	preferenceService      pref.Service
	ds                     dashboards.DashboardService
}

func ProvideTeamAPI(
	routeRegister routing.RouteRegister,
	teamService team.Service,
	ac accesscontrol.Service,
	acEvaluator accesscontrol.AccessControl,
	teamPermissionsService accesscontrol.TeamPermissionsService,
	license licensing.Licensing,
	cfg *setting.Cfg,
	preferenceService pref.Service,
	ds dashboards.DashboardService,
) *TeamAPI {
	tapi := &TeamAPI{
		teamService:            teamService,
		ac:                     ac,
		teamPermissionsService: teamPermissionsService,
		license:                license,
		cfg:                    cfg,
		preferenceService:      preferenceService,
		ds:                     ds,
	}

	tapi.registerRoutes(routeRegister, acEvaluator)
	return tapi
}

func (tapi *TeamAPI) registerRoutes(router routing.RouteRegister, ac accesscontrol.AccessControl) {
	authorize := accesscontrol.Middleware(ac)
	router.Group("/api", func(apiRoute routing.RouteRegister) {
		// team (admin permission required)
		apiRoute.Group("/teams", func(teamsRoute routing.RouteRegister) {
			teamsRoute.Post("/", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsCreate)),
				routing.Wrap(tapi.createTeam))
			teamsRoute.Put("/:teamId", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsWrite,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.updateTeam))
			teamsRoute.Delete("/:teamId", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsDelete,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.deleteTeamByID))
			teamsRoute.Get("/:teamId/members", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsPermissionsRead,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.getTeamMembers))
			teamsRoute.Post("/:teamId/members", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsPermissionsWrite,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.addTeamMember))
			teamsRoute.Put("/:teamId/members/:userId", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsPermissionsWrite,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.updateTeamMember))
			teamsRoute.Delete("/:teamId/members/:userId", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsPermissionsWrite,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.removeTeamMember))
			teamsRoute.Get("/:teamId/preferences", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsRead,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.getTeamPreferences))
			teamsRoute.Put("/:teamId/preferences", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsWrite,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.updateTeamPreferences))
		}, requestmeta.SetOwner(requestmeta.TeamAuth))

		// team without requirement of user to be org admin
		apiRoute.Group("/teams", func(teamsRoute routing.RouteRegister) {
			teamsRoute.Get("/:teamId", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsRead,
				accesscontrol.ScopeTeamsID)), routing.Wrap(tapi.getTeamByID))
			teamsRoute.Get("/search", authorize(accesscontrol.EvalPermission(accesscontrol.ActionTeamsRead)),
				routing.Wrap(tapi.searchTeams))
		}, requestmeta.SetOwner(requestmeta.TeamAuth))
	})
}
