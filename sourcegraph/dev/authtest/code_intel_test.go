package authtest

import (
	"bytes"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/gqltestutil"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestCodeIntelEndpoints(t *testing.T) {
	// Create a test user (authtest-user-code-intel) which is not a site admin, the
	// user should receive access denied for LSIF endpoints of repositories the user
	// does not have access to.
	const testUsername = "authtest-user-code-intel"
	userClient, err := gqltestutil.SignUp(*baseURL, testUsername+"@sourcegraph.com", testUsername, "mysecurepassword")
	if err != nil {
		t.Fatal(err)
	}
	defer func() {
		err := client.DeleteUser(userClient.AuthenticatedUserID(), true)
		if err != nil {
			t.Fatal(err)
		}
	}()

	// Set up external service
	esID, err := client.AddExternalService(
		gqltestutil.AddExternalServiceInput{
			Kind:        extsvc.KindGitHub,
			DisplayName: "authtest-github-code-intel-repository",
			Config: mustMarshalJSONString(
				&schema.GitHubConnection{
					Authorization: &schema.GitHubAuthorization{},
					Repos: []string{
						"sgtest/go-diff",
						"sgtest/private", // Private
					},
					RepositoryPathPattern: "github.com/{nameWithOwner}",
					Token:                 *githubToken,
					Url:                   "https://ghe.sgdev.org/",
				},
			),
		},
	)
	if err != nil {
		t.Fatal(err)
	}
	defer func() {
		err := client.DeleteExternalService(esID, false)
		if err != nil {
			t.Fatal(err)
		}
	}()

	const privateRepo = "github.com/sgtest/private"
	err = client.WaitForReposToBeCloned(
		"github.com/sgtest/go-diff",
		privateRepo,
	)
	if err != nil {
		t.Fatal(err)
	}

	t.Run("SCIP upload", func(t *testing.T) {
		// Update site configuration to enable "lsifEnforceAuth".
		siteConfig, lastID, err := client.SiteConfiguration()
		if err != nil {
			t.Fatal(err)
		}

		oldSiteConfig := new(schema.SiteConfiguration)
		*oldSiteConfig = *siteConfig
		defer func() {
			_, lastID, err := client.SiteConfiguration()
			if err != nil {
				t.Fatal(err)
			}
			err = client.UpdateSiteConfiguration(oldSiteConfig, lastID)
			if err != nil {
				t.Fatal(err)
			}
		}()

		siteConfig.LsifEnforceAuth = true
		err = client.UpdateSiteConfiguration(siteConfig, lastID)
		if err != nil {
			t.Fatal(err)
		}

		// Retry because the configuration update endpoint is eventually consistent
		var lastBody string
		err = gqltestutil.Retry(15*time.Second, func() error {
			resp, err := userClient.Post(*baseURL+"/.api/scip/upload?commit=6ffc6072f5ed13d8e8782490705d9689cd2c546a&repository=github.com/sgtest/private", nil)
			if err != nil {
				t.Fatal(err)
			}
			defer func() { _ = resp.Body.Close() }()

			body, err := io.ReadAll(resp.Body)
			if err != nil {
				t.Fatal(err)
			}

			if bytes.Contains(body, []byte("must provide github_token")) {
				return nil
			}

			lastBody = string(body)
			return gqltestutil.ErrContinueRetry
		})
		if err != nil {
			t.Fatal(err, "lastBody:", lastBody)
		}
	})

	t.Run("executor endpoints (access token not configured)", func(t *testing.T) {
		// Update site configuration to remove any executor access token.
		cleanup := setExecutorAccessToken(t, "")
		defer cleanup()

		resp, err := userClient.Get(*baseURL + "/.executors/test/auth")
		if err != nil {
			t.Fatal(err)
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode != http.StatusInternalServerError {
			t.Fatalf(`Want status code 500 error but got %d`, resp.StatusCode)
		}

		response, err := io.ReadAll(resp.Body)
		if err != nil {
			t.Fatal(err)
		}
		expectedText := "Executors are not configured on this instance"
		if !strings.Contains(string(response), expectedText) {
			t.Fatalf(`Expected different failure. want=%q got=%q`, expectedText, string(response))
		}
	})

	t.Run("executor endpoints (access token configured but not supplied)", func(t *testing.T) {
		// Update site configuration to set the executor access token.
		cleanup := setExecutorAccessToken(t, "hunter2hunter2hunter2")
		defer cleanup()

		// sleep 5s to wait for site configuration to be restored from gqltest
		time.Sleep(5 * time.Second)

		resp, err := userClient.Get(*baseURL + "/.executors/test/auth")
		if err != nil {
			t.Fatal(err)
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode != http.StatusUnauthorized {
			t.Fatalf(`Want status code 401 error but got %d`, resp.StatusCode)
		}
	})
}

func setExecutorAccessToken(t *testing.T, token string) func() {
	siteConfig, lastID, err := client.SiteConfiguration()
	if err != nil {
		t.Fatal(err)
	}

	oldSiteConfig := new(schema.SiteConfiguration)
	*oldSiteConfig = *siteConfig
	siteConfig.ExecutorsAccessToken = token

	if err := client.UpdateSiteConfiguration(siteConfig, lastID); err != nil {
		t.Fatal(err)
	}
	return func() {
		_, lastID, err := client.SiteConfiguration()
		if err != nil {
			t.Fatal(err)
		}
		if err := client.UpdateSiteConfiguration(oldSiteConfig, lastID); err != nil {
			t.Fatal(err)
		}
	}
}
