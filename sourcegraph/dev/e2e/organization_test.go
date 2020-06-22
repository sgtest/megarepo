// +build e2e

package main

import (
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	jsoniter "github.com/json-iterator/go"

	"github.com/sourcegraph/sourcegraph/internal/e2eutil"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestOrganization(t *testing.T) {
	const testOrgName = "e2e-test-org"
	orgID, err := client.CreateOrganization(testOrgName, testOrgName)
	if err != nil {
		t.Fatal(err)
	}
	defer func() {
		err := client.DeleteOrganization(orgID)
		if err != nil {
			t.Fatal(err)
		}
	}()

	t.Run("settings cascade", func(t *testing.T) {
		err := client.OverwriteSettings(orgID, `{"quicklinks":[{"name":"Test quicklink","url":"http://test-quicklink.local"}]}`)
		if err != nil {
			t.Fatal(err)
		}
		defer func() {
			err := client.OverwriteSettings(orgID, `{}`)
			if err != nil {
				t.Fatal(err)
			}
		}()

		{
			contents, err := client.ViewerSettings()
			if err != nil {
				t.Fatal(err)
			}

			var got struct {
				QuickLinks []schema.QuickLink `json:"quicklinks"`
			}
			err = jsoniter.UnmarshalFromString(contents, &got)
			if err != nil {
				t.Fatal(err)
			}

			wantQuickLinks := []schema.QuickLink{
				{
					Name: "Test quicklink",
					Url:  "http://test-quicklink.local",
				},
			}
			if diff := cmp.Diff(wantQuickLinks, got.QuickLinks); diff != "" {
				t.Fatalf("QuickLinks mismatch (-want +got):\n%s", diff)
			}
		}

		// Remove authenticate user (e2e-admin) from organization (e2e-test-org) should
		// no longer get cascaded settings from this organization.
		err = client.RemoveUserFromOrganization(client.AuthenticatedUserID(), orgID)
		if err != nil {
			t.Fatal(err)
		}

		{
			contents, err := client.ViewerSettings()
			if err != nil {
				t.Fatal(err)
			}

			var got struct {
				QuickLinks []schema.QuickLink `json:"quicklinks"`
			}
			err = jsoniter.UnmarshalFromString(contents, &got)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff([]schema.QuickLink(nil), got.QuickLinks); diff != "" {
				t.Fatalf("QuickLinks mismatch (-want +got):\n%s", diff)
			}
		}
	})

	// Docs: https://docs.sourcegraph.com/user/organizations
	t.Run("auth.userOrgMap", func(t *testing.T) {
		// Create a test user (test-org-user-1) without settings "auth.userOrgMap",
		// the user should not be added to the organization (e2e-test-org) automatically.
		const testUsername1 = "test-org-user-1"
		testUserID1, err := client.CreateUser(testUsername1, testUsername1+"@sourcegraph.com")
		if err != nil {
			t.Fatal(err)
		}
		defer func() {
			err := client.DeleteUser(testUserID1, true)
			if err != nil {
				t.Fatal(err)
			}
		}()

		orgs, err := client.UserOrganizations(testUsername1)
		if err != nil {
			t.Fatal(err)
		}

		if diff := cmp.Diff([]string{}, orgs); diff != "" {
			t.Fatalf("Organizations mismatch (-want +got):\n%s", diff)
		}

		// Update site configuration to set "auth.userOrgMap" which makes the new user join
		// the organization (e2e-test-org) automatically.
		siteConfig, err := client.SiteConfiguration()
		if err != nil {
			t.Fatal(err)
		}
		oldSiteConfig := new(schema.SiteConfiguration)
		*oldSiteConfig = *siteConfig
		defer func() {
			err = client.UpdateSiteConfiguration(oldSiteConfig)
			if err != nil {
				t.Fatal(err)
			}
		}()

		siteConfig.AuthUserOrgMap = map[string][]string{"*": {testOrgName}}
		err = client.UpdateSiteConfiguration(siteConfig)
		if err != nil {
			t.Fatal(err)
		}

		var lastOrgs []string
		// Retry because the configuration update endpoint is eventually consistent
		err = e2eutil.Retry(5*time.Second, func() error {
			// Create another test user (test-org-user-2) and the user should be added to
			// the organization (e2e-test-org) automatically.
			const testUsername2 = "test-org-user-2"
			testUserID2, err := client.CreateUser(testUsername2, testUsername2+"@sourcegraph.com")
			if err != nil {
				t.Fatal(err)
			}
			defer func() {
				err := client.DeleteUser(testUserID2, true)
				if err != nil {
					t.Fatal(err)
				}
			}()

			orgs, err = client.UserOrganizations(testUsername2)
			if err != nil {
				t.Fatal(err)
			}
			lastOrgs = orgs

			wantOrgs := []string{testOrgName}
			if cmp.Diff(wantOrgs, orgs) != "" {
				return e2eutil.ErrContinueRetry
			}
			return nil
		})
		if err != nil {
			t.Fatal(err, "lastOrgs:", lastOrgs)
		}
	})
}
