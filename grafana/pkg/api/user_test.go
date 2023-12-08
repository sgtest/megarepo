package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.org/x/oauth2"

	"github.com/grafana/grafana/pkg/api/dtos"
	"github.com/grafana/grafana/pkg/api/response"
	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/components/simplejson"
	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/db/dbtest"
	"github.com/grafana/grafana/pkg/login/social"
	"github.com/grafana/grafana/pkg/login/social/socialtest"
	"github.com/grafana/grafana/pkg/services/accesscontrol/acimpl"
	acmock "github.com/grafana/grafana/pkg/services/accesscontrol/mock"
	contextmodel "github.com/grafana/grafana/pkg/services/contexthandler/model"
	"github.com/grafana/grafana/pkg/services/login"
	"github.com/grafana/grafana/pkg/services/login/authinfoimpl"
	"github.com/grafana/grafana/pkg/services/login/authinfotest"
	"github.com/grafana/grafana/pkg/services/org/orgimpl"
	"github.com/grafana/grafana/pkg/services/quota/quotatest"
	"github.com/grafana/grafana/pkg/services/searchusers"
	"github.com/grafana/grafana/pkg/services/searchusers/filters"
	"github.com/grafana/grafana/pkg/services/secrets/database"
	secretsManager "github.com/grafana/grafana/pkg/services/secrets/manager"
	"github.com/grafana/grafana/pkg/services/supportbundles/supportbundlestest"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/services/user/userimpl"
	"github.com/grafana/grafana/pkg/services/user/usertest"
	"github.com/grafana/grafana/pkg/setting"
)

func TestUserAPIEndpoint_userLoggedIn(t *testing.T) {
	settings := setting.NewCfg()
	sqlStore := db.InitTestDB(t)
	sqlStore.Cfg = settings
	hs := &HTTPServer{
		Cfg:           settings,
		SQLStore:      sqlStore,
		AccessControl: acimpl.ProvideAccessControl(settings),
	}

	mockResult := user.SearchUserQueryResult{
		Users: []*user.UserSearchHitDTO{
			{Name: "user1"},
			{Name: "user2"},
		},
		TotalCount: 2,
	}
	mock := dbtest.NewFakeDB()
	userMock := usertest.NewUserServiceFake()

	loggedInUserScenario(t, "When calling GET on", "api/users/1", "api/users/:id", func(sc *scenarioContext) {
		fakeNow := time.Date(2019, 2, 11, 17, 30, 40, 0, time.UTC)
		secretsService := secretsManager.SetupTestService(t, database.ProvideSecretsStore(sqlStore))
		authInfoStore := authinfoimpl.ProvideStore(sqlStore, secretsService)
		srv := authinfoimpl.ProvideService(
			authInfoStore,
		)
		hs.authInfoService = srv
		orgSvc, err := orgimpl.ProvideService(sqlStore, sqlStore.Cfg, quotatest.New(false, nil))
		require.NoError(t, err)
		require.NoError(t, err)
		userSvc, err := userimpl.ProvideService(sqlStore, orgSvc, sc.cfg, nil, nil, quotatest.New(false, nil), supportbundlestest.NewFakeBundleService())
		require.NoError(t, err)
		hs.userService = userSvc

		createUserCmd := user.CreateUserCommand{
			Email:   fmt.Sprint("user", "@test.com"),
			Name:    "user",
			Login:   "loginuser",
			IsAdmin: true,
		}
		usr, err := userSvc.Create(context.Background(), &createUserCmd)
		require.NoError(t, err)

		sc.handlerFunc = hs.GetUserByID

		token := &oauth2.Token{
			AccessToken:  "testaccess",
			RefreshToken: "testrefresh",
			Expiry:       time.Now(),
			TokenType:    "Bearer",
		}
		idToken := "testidtoken"
		token = token.WithExtra(map[string]any{"id_token": idToken})
		userlogin := "loginuser"
		query := &login.GetUserByAuthInfoQuery{AuthModule: "test", AuthId: "test", UserLookupParams: login.UserLookupParams{Login: &userlogin}}
		cmd := &login.UpdateAuthInfoCommand{
			UserId:     usr.ID,
			AuthId:     query.AuthId,
			AuthModule: query.AuthModule,
			OAuthToken: token,
		}
		err = srv.UpdateAuthInfo(context.Background(), cmd)
		require.NoError(t, err)
		avatarUrl := dtos.GetGravatarUrl("@test.com")
		sc.fakeReqWithParams("GET", sc.url, map[string]string{"id": fmt.Sprintf("%v", usr.ID)}).exec()

		expected := user.UserProfileDTO{
			ID:             1,
			Email:          "user@test.com",
			Name:           "user",
			Login:          "loginuser",
			OrgID:          1,
			IsGrafanaAdmin: true,
			AuthLabels:     []string{},
			CreatedAt:      fakeNow,
			UpdatedAt:      fakeNow,
			AvatarURL:      avatarUrl,
		}

		var resp user.UserProfileDTO
		require.Equal(t, http.StatusOK, sc.resp.Code)
		err = json.Unmarshal(sc.resp.Body.Bytes(), &resp)
		require.NoError(t, err)
		resp.CreatedAt = fakeNow
		resp.UpdatedAt = fakeNow
		resp.AvatarURL = avatarUrl
		require.EqualValues(t, expected, resp)
	}, mock)

	loggedInUserScenario(t, "When calling GET on", "/api/users/lookup", "/api/users/lookup", func(sc *scenarioContext) {
		createUserCmd := user.CreateUserCommand{
			Email:   fmt.Sprint("admin", "@test.com"),
			Name:    "admin",
			Login:   "admin",
			IsAdmin: true,
		}
		orgSvc, err := orgimpl.ProvideService(sqlStore, sqlStore.Cfg, quotatest.New(false, nil))
		require.NoError(t, err)
		userSvc, err := userimpl.ProvideService(sqlStore, orgSvc, sc.cfg, nil, nil, quotatest.New(false, nil), supportbundlestest.NewFakeBundleService())
		require.NoError(t, err)
		_, err = userSvc.Create(context.Background(), &createUserCmd)
		require.Nil(t, err)

		sc.handlerFunc = hs.GetUserByLoginOrEmail

		userMock := usertest.NewUserServiceFake()
		userMock.ExpectedUser = &user.User{ID: 2}
		sc.userService = userMock
		hs.userService = userMock
		sc.fakeReqWithParams("GET", sc.url, map[string]string{"loginOrEmail": "admin@test.com"}).exec()

		var resp user.UserProfileDTO
		require.Equal(t, http.StatusOK, sc.resp.Code)
		err = json.Unmarshal(sc.resp.Body.Bytes(), &resp)
		require.NoError(t, err)
	}, mock)

	loggedInUserScenario(t, "When calling GET on", "/api/users", "/api/users", func(sc *scenarioContext) {
		userMock.ExpectedSearchUsers = mockResult

		searchUsersService := searchusers.ProvideUsersService(filters.ProvideOSSSearchUserFilter(), userMock)
		sc.handlerFunc = searchUsersService.SearchUsers
		sc.fakeReqWithParams("GET", sc.url, map[string]string{}).exec()

		respJSON, err := simplejson.NewJson(sc.resp.Body.Bytes())
		require.NoError(t, err)

		assert.Equal(t, 2, len(respJSON.MustArray()))
	}, mock)

	loggedInUserScenario(t, "When calling GET with page and limit querystring parameters on", "/api/users", "/api/users", func(sc *scenarioContext) {
		userMock.ExpectedSearchUsers = mockResult

		searchUsersService := searchusers.ProvideUsersService(filters.ProvideOSSSearchUserFilter(), userMock)
		sc.handlerFunc = searchUsersService.SearchUsers
		sc.fakeReqWithParams("GET", sc.url, map[string]string{"perpage": "10", "page": "2"}).exec()

		respJSON, err := simplejson.NewJson(sc.resp.Body.Bytes())
		require.NoError(t, err)

		assert.Equal(t, 2, len(respJSON.MustArray()))
	}, mock)

	loggedInUserScenario(t, "When calling GET on", "/api/users/search", "/api/users/search", func(sc *scenarioContext) {
		userMock.ExpectedSearchUsers = mockResult

		searchUsersService := searchusers.ProvideUsersService(filters.ProvideOSSSearchUserFilter(), userMock)
		sc.handlerFunc = searchUsersService.SearchUsersWithPaging
		sc.fakeReqWithParams("GET", sc.url, map[string]string{}).exec()

		respJSON, err := simplejson.NewJson(sc.resp.Body.Bytes())
		require.NoError(t, err)

		assert.Equal(t, 1, respJSON.Get("page").MustInt())
		assert.Equal(t, 1000, respJSON.Get("perPage").MustInt())
		assert.Equal(t, 2, respJSON.Get("totalCount").MustInt())
		assert.Equal(t, 2, len(respJSON.Get("users").MustArray()))
	}, mock)

	loggedInUserScenario(t, "When calling GET with page and perpage querystring parameters on", "/api/users/search", "/api/users/search", func(sc *scenarioContext) {
		userMock.ExpectedSearchUsers = mockResult

		searchUsersService := searchusers.ProvideUsersService(filters.ProvideOSSSearchUserFilter(), userMock)
		sc.handlerFunc = searchUsersService.SearchUsersWithPaging
		sc.fakeReqWithParams("GET", sc.url, map[string]string{"perpage": "10", "page": "2"}).exec()

		respJSON, err := simplejson.NewJson(sc.resp.Body.Bytes())
		require.NoError(t, err)

		assert.Equal(t, 2, respJSON.Get("page").MustInt())
		assert.Equal(t, 10, respJSON.Get("perPage").MustInt())
	}, mock)
}

func Test_GetUserByID(t *testing.T) {
	testcases := []struct {
		name                         string
		authModule                   string
		allowAssignGrafanaAdmin      bool
		authEnabled                  bool
		skipOrgRoleSync              bool
		expectedIsGrafanaAdminSynced bool
	}{
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = false for an externally synced OAuth user if Grafana Admin role is not synced",
			authModule:                   login.GenericOAuthModule,
			authEnabled:                  true,
			allowAssignGrafanaAdmin:      false,
			skipOrgRoleSync:              false,
			expectedIsGrafanaAdminSynced: false,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = false for an externally synced OAuth user if OAuth provider is not enabled",
			authModule:                   login.GenericOAuthModule,
			authEnabled:                  false,
			allowAssignGrafanaAdmin:      true,
			skipOrgRoleSync:              false,
			expectedIsGrafanaAdminSynced: false,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = false for an externally synced OAuth user if org roles are not being synced",
			authModule:                   login.GenericOAuthModule,
			authEnabled:                  true,
			allowAssignGrafanaAdmin:      true,
			skipOrgRoleSync:              true,
			expectedIsGrafanaAdminSynced: false,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = true for an externally synced OAuth user",
			authModule:                   login.GenericOAuthModule,
			authEnabled:                  true,
			allowAssignGrafanaAdmin:      true,
			skipOrgRoleSync:              false,
			expectedIsGrafanaAdminSynced: true,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = false for an externally synced JWT user if Grafana Admin role is not synced",
			authModule:                   login.JWTModule,
			authEnabled:                  true,
			allowAssignGrafanaAdmin:      false,
			skipOrgRoleSync:              false,
			expectedIsGrafanaAdminSynced: false,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = false for an externally synced JWT user if JWT provider is not enabled",
			authModule:                   login.JWTModule,
			authEnabled:                  false,
			allowAssignGrafanaAdmin:      true,
			skipOrgRoleSync:              false,
			expectedIsGrafanaAdminSynced: false,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = false for an externally synced JWT user if org roles are not being synced",
			authModule:                   login.JWTModule,
			authEnabled:                  true,
			allowAssignGrafanaAdmin:      true,
			skipOrgRoleSync:              true,
			expectedIsGrafanaAdminSynced: false,
		},
		{
			name:                         "Should return IsGrafanaAdminExternallySynced = true for an externally synced JWT user",
			authModule:                   login.JWTModule,
			authEnabled:                  true,
			allowAssignGrafanaAdmin:      true,
			skipOrgRoleSync:              false,
			expectedIsGrafanaAdminSynced: true,
		},
	}
	for _, tc := range testcases {
		t.Run(tc.name, func(t *testing.T) {
			userAuth := &login.UserAuth{AuthModule: tc.authModule}
			authInfoService := &authinfotest.FakeService{ExpectedUserAuth: userAuth}
			socialService := &socialtest.FakeSocialService{}
			userService := &usertest.FakeUserService{ExpectedUserProfileDTO: &user.UserProfileDTO{}}
			cfg := setting.NewCfg()

			switch tc.authModule {
			case login.GenericOAuthModule:
				socialService.ExpectedAuthInfoProvider = &social.OAuthInfo{AllowAssignGrafanaAdmin: tc.allowAssignGrafanaAdmin, Enabled: tc.authEnabled}
				cfg.GenericOAuthAuthEnabled = tc.authEnabled
				cfg.GenericOAuthSkipOrgRoleSync = tc.skipOrgRoleSync
			case login.JWTModule:
				cfg.JWTAuthEnabled = tc.authEnabled
				cfg.JWTAuthSkipOrgRoleSync = tc.skipOrgRoleSync
				cfg.JWTAuthAllowAssignGrafanaAdmin = tc.allowAssignGrafanaAdmin
			}

			hs := &HTTPServer{
				Cfg:             cfg,
				authInfoService: authInfoService,
				SocialService:   socialService,
				userService:     userService,
			}

			sc := setupScenarioContext(t, "/api/users/1")
			sc.defaultHandler = routing.Wrap(func(c *contextmodel.ReqContext) response.Response {
				sc.context = c
				return hs.GetUserByID(c)
			})

			sc.m.Get("/api/users/:id", sc.defaultHandler)
			sc.fakeReqWithParams("GET", sc.url, map[string]string{}).exec()

			var resp user.UserProfileDTO
			require.Equal(t, http.StatusOK, sc.resp.Code)
			err := json.Unmarshal(sc.resp.Body.Bytes(), &resp)
			require.NoError(t, err)

			assert.Equal(t, tc.expectedIsGrafanaAdminSynced, resp.IsGrafanaAdminExternallySynced)
		})
	}
}

func TestHTTPServer_UpdateUser(t *testing.T) {
	settings := setting.NewCfg()
	sqlStore := db.InitTestDB(t)

	hs := &HTTPServer{
		Cfg:           settings,
		SQLStore:      sqlStore,
		AccessControl: acmock.New(),
	}

	updateUserCommand := user.UpdateUserCommand{
		Email:  fmt.Sprint("admin", "@test.com"),
		Name:   "admin",
		Login:  "admin",
		UserID: 1,
	}

	updateUserScenario(t, updateUserContext{
		desc:         "Should return 403 when the current User is an external user",
		url:          "/api/users/1",
		routePattern: "/api/users/:id",
		cmd:          updateUserCommand,
		fn: func(sc *scenarioContext) {
			sc.authInfoService.ExpectedUserAuth = &login.UserAuth{}
			sc.fakeReqWithParams("PUT", sc.url, map[string]string{"id": "1"}).exec()
			assert.Equal(t, 403, sc.resp.Code)
		},
	}, hs)
}

type updateUserContext struct {
	desc         string
	url          string
	routePattern string
	cmd          user.UpdateUserCommand
	fn           scenarioFunc
}

func updateUserScenario(t *testing.T, ctx updateUserContext, hs *HTTPServer) {
	t.Run(fmt.Sprintf("%s %s", ctx.desc, ctx.url), func(t *testing.T) {
		sc := setupScenarioContext(t, ctx.url)

		sc.authInfoService = &authinfotest.FakeService{}
		hs.authInfoService = sc.authInfoService

		sc.defaultHandler = routing.Wrap(func(c *contextmodel.ReqContext) response.Response {
			c.Req.Body = mockRequestBody(ctx.cmd)
			c.Req.Header.Add("Content-Type", "application/json")
			sc.context = c
			sc.context.OrgID = testOrgID
			sc.context.UserID = testUserID

			return hs.UpdateUser(c)
		})

		sc.m.Put(ctx.routePattern, sc.defaultHandler)

		ctx.fn(sc)
	})
}

func TestHTTPServer_UpdateSignedInUser(t *testing.T) {
	settings := setting.NewCfg()
	sqlStore := db.InitTestDB(t)

	hs := &HTTPServer{
		Cfg:           settings,
		SQLStore:      sqlStore,
		AccessControl: acmock.New(),
	}

	updateUserCommand := user.UpdateUserCommand{
		Email:  fmt.Sprint("admin", "@test.com"),
		Name:   "admin",
		Login:  "admin",
		UserID: 1,
	}

	updateSignedInUserScenario(t, updateUserContext{
		desc:         "Should return 403 when the current User is an external user",
		url:          "/api/users/",
		routePattern: "/api/users/",
		cmd:          updateUserCommand,
		fn: func(sc *scenarioContext) {
			sc.authInfoService.ExpectedUserAuth = &login.UserAuth{}
			sc.fakeReqWithParams("PUT", sc.url, map[string]string{"id": "1"}).exec()
			assert.Equal(t, 403, sc.resp.Code)
		},
	}, hs)
}

func updateSignedInUserScenario(t *testing.T, ctx updateUserContext, hs *HTTPServer) {
	t.Run(fmt.Sprintf("%s %s", ctx.desc, ctx.url), func(t *testing.T) {
		sc := setupScenarioContext(t, ctx.url)

		sc.authInfoService = &authinfotest.FakeService{}
		hs.authInfoService = sc.authInfoService

		sc.defaultHandler = routing.Wrap(func(c *contextmodel.ReqContext) response.Response {
			c.Req.Body = mockRequestBody(ctx.cmd)
			c.Req.Header.Add("Content-Type", "application/json")
			sc.context = c
			sc.context.OrgID = testOrgID
			sc.context.UserID = testUserID

			return hs.UpdateSignedInUser(c)
		})

		sc.m.Put(ctx.routePattern, sc.defaultHandler)

		ctx.fn(sc)
	})
}
