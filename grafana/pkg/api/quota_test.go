package api

import (
	"fmt"
	"net/http"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/accesscontrol/actest"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/services/user/usertest"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/web/webtest"
)

var (
	getCurrentOrgQuotasURL = "/api/org/quotas"
	getOrgsQuotasURL       = "/api/orgs/%v/quotas"
	putOrgsQuotasURL       = "/api/orgs/%v/quotas/%v"

	testUpdateOrgQuotaCmd = `{ "limit": 20 }`
)

func TestAPIEndpoint_GetCurrentOrgQuotas(t *testing.T) {
	cfg := setting.NewCfg()
	cfg.Quota.Enabled = true
	server := SetupAPITestServer(t, func(hs *HTTPServer) {
		hs.Cfg = cfg
	})

	t.Run("AccessControl allows viewing CurrentOrgQuotas with correct permissions", func(t *testing.T) {
		req := webtest.RequestWithSignedInUser(server.NewGetRequest(getCurrentOrgQuotasURL), userWithPermissions(1, []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasRead}}))
		res, err := server.Send(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusOK, res.StatusCode)
		require.NoError(t, res.Body.Close())
	})
	t.Run("AccessControl prevents viewing CurrentOrgQuotas with correct permissions in another org", func(t *testing.T) {
		// Set permissions in org 2, but set current org to org 1
		user := userWithPermissions(2, []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasRead}})
		user.OrgID = 1
		req := webtest.RequestWithSignedInUser(server.NewGetRequest(getCurrentOrgQuotasURL), user)
		res, err := server.Send(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusForbidden, res.StatusCode)
		require.NoError(t, res.Body.Close())
	})
	t.Run("AccessControl prevents viewing CurrentOrgQuotas with incorrect permissions", func(t *testing.T) {
		req := webtest.RequestWithSignedInUser(server.NewGetRequest(getCurrentOrgQuotasURL), userWithPermissions(1, []accesscontrol.Permission{{Action: "orgs:invalid"}}))
		res, err := server.Send(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusForbidden, res.StatusCode)
		require.NoError(t, res.Body.Close())
	})
}

func TestAPIEndpoint_GetOrgQuotas(t *testing.T) {
	cfg := setting.NewCfg()
	cfg.Quota.Enabled = true
	server := SetupAPITestServer(t, func(hs *HTTPServer) {
		hs.Cfg = cfg
		hs.accesscontrolService = &actest.FakeService{ExpectedPermissions: []accesscontrol.Permission{}}
		hs.userService = &usertest.FakeUserService{
			ExpectedSignedInUser: &user.SignedInUser{OrgID: 2},
		}
	})

	t.Run("AccessControl allows viewing another org quotas with correct permissions", func(t *testing.T) {
		req := webtest.RequestWithSignedInUser(server.NewGetRequest(fmt.Sprintf(getOrgsQuotasURL, 2)), userWithPermissions(2, []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasRead}}))
		res, err := server.Send(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusOK, res.StatusCode)
		require.NoError(t, res.Body.Close())
	})
	t.Run("AccessControl prevents viewing another org quotas with correct permissions in another org", func(t *testing.T) {
		// Set correct permissions in org 1 and empty permissions in org 2
		user := userWithPermissions(1, []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasRead}})
		user.Permissions[2] = nil
		req := webtest.RequestWithSignedInUser(server.NewGetRequest(fmt.Sprintf(getOrgsQuotasURL, 2)), user)
		res, err := server.Send(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusForbidden, res.StatusCode)
		require.NoError(t, res.Body.Close())
	})
	t.Run("AccessControl prevents viewing another org quotas with incorrect permissions", func(t *testing.T) {
		req := webtest.RequestWithSignedInUser(server.NewGetRequest(fmt.Sprintf(getOrgsQuotasURL, 2)), userWithPermissions(2, []accesscontrol.Permission{{Action: "orgs:invalid"}}))
		res, err := server.Send(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusForbidden, res.StatusCode)
		require.NoError(t, res.Body.Close())
	})
}

func TestAPIEndpoint_PutOrgQuotas(t *testing.T) {
	cfg := setting.NewCfg()
	cfg.Quota = setting.QuotaSettings{
		Enabled: true,
		Global: setting.GlobalQuota{
			Org: 5,
		},
		Org: setting.OrgQuota{
			User: 5,
		},
		User: setting.UserQuota{
			Org: 5,
		},
	}
	fakeACService := &actest.FakeService{}
	server := SetupAPITestServer(t, func(hs *HTTPServer) {
		hs.Cfg = cfg
		hs.accesscontrolService = fakeACService
		hs.userService = &usertest.FakeUserService{
			ExpectedSignedInUser: &user.SignedInUser{OrgID: 2},
		}
	})

	input := strings.NewReader(testUpdateOrgQuotaCmd)
	t.Run("AccessControl allows updating another org quotas with correct permissions", func(t *testing.T) {
		user := userWithPermissions(2, []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasWrite}})
		user.OrgID = 1
		fakeACService.ExpectedPermissions = []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasWrite}}
		req := webtest.RequestWithSignedInUser(server.NewRequest(http.MethodPut, fmt.Sprintf(putOrgsQuotasURL, 2, "org_user"), input), user)
		response, err := server.SendJSON(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusOK, response.StatusCode)
		require.NoError(t, response.Body.Close())
	})

	input = strings.NewReader(testUpdateOrgQuotaCmd)
	t.Run("AccessControl prevents updating another org quotas with correct permissions in another org", func(t *testing.T) {
		user := userWithPermissions(1, []accesscontrol.Permission{{Action: accesscontrol.ActionOrgsQuotasWrite}})
		user.Permissions[2] = nil
		fakeACService.ExpectedPermissions = []accesscontrol.Permission{}
		req := webtest.RequestWithSignedInUser(server.NewRequest(http.MethodPut, fmt.Sprintf(putOrgsQuotasURL, 2, "org_user"), input), user)
		response, err := server.SendJSON(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusForbidden, response.StatusCode)
		require.NoError(t, response.Body.Close())
	})

	input = strings.NewReader(testUpdateOrgQuotaCmd)
	t.Run("AccessControl prevents updating another org quotas with incorrect permissions", func(t *testing.T) {
		user := userWithPermissions(2, []accesscontrol.Permission{{Action: "orgs:invalid"}})
		fakeACService.ExpectedPermissions = []accesscontrol.Permission{}
		req := webtest.RequestWithSignedInUser(server.NewRequest(http.MethodPut, fmt.Sprintf(putOrgsQuotasURL, 2, "org_user"), input), user)
		response, err := server.SendJSON(req)
		require.NoError(t, err)
		assert.Equal(t, http.StatusForbidden, response.StatusCode)
		require.NoError(t, response.Body.Close())
	})
}
