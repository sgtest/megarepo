package main

import (
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/gqltestutil"
)

func TestExternalService(t *testing.T) {
	if len(*githubToken) == 0 {
		t.Skip("Environment variable GITHUB_TOKEN is not set")
	}

	t.Run("repositoryPathPattern", func(t *testing.T) {
		const repo = "sgtest/go-diff" // Tiny repo, fast to clone
		const slug = "github.com/" + repo
		// Set up external service
		esID, err := client.AddExternalService(gqltestutil.AddExternalServiceInput{
			Kind:        extsvc.KindGitHub,
			DisplayName: "gqltest-github-repoPathPattern",
			Config: mustMarshalJSONString(struct {
				URL                   string   `json:"url"`
				Token                 string   `json:"token"`
				Repos                 []string `json:"repos"`
				RepositoryPathPattern string   `json:"repositoryPathPattern"`
			}{
				URL:                   "https://ghe.sgdev.org/",
				Token:                 *githubToken,
				Repos:                 []string{repo},
				RepositoryPathPattern: "github.com/{nameWithOwner}",
			}),
		})
		// The repo-updater might not be up yet, but it will eventually catch up for the external
		// service we just added, thus it is OK to ignore this transient error.
		if err != nil && !strings.Contains(err.Error(), "/sync-external-service") {
			t.Fatal(err)
		}
		defer func() {
			err := client.DeleteExternalService(esID, false)
			if err != nil {
				t.Fatal(err)
			}
		}()

		err = client.WaitForReposToBeCloned(slug)
		if err != nil {
			t.Fatal(err)
		}

		// The request URL should be redirected to the new path
		origURL := *baseURL + "/" + slug
		resp, err := client.Get(origURL)
		if err != nil {
			t.Fatal(err)
		}
		defer func() { _ = resp.Body.Close() }()

		wantURL := *baseURL + "/" + slug // <baseURL>/github.com/sgtest/go-diff
		if diff := cmp.Diff(wantURL, resp.Request.URL.String()); diff != "" {
			t.Fatalf("URL mismatch (-want +got):\n%s", diff)
		}
	})
}

func TestExternalService_AWSCodeCommit(t *testing.T) {
	if len(*awsAccessKeyID) == 0 || len(*awsSecretAccessKey) == 0 ||
		len(*awsCodeCommitUsername) == 0 || len(*awsCodeCommitPassword) == 0 {
		t.Skip("Environment variable AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_CODE_COMMIT_USERNAME or AWS_CODE_COMMIT_PASSWORD is not set")
	}

	// Set up external service
	esID, err := client.AddExternalService(gqltestutil.AddExternalServiceInput{
		Kind:        extsvc.KindAWSCodeCommit,
		DisplayName: "gqltest-aws-code-commit",
		Config: mustMarshalJSONString(struct {
			Region                string            `json:"region"`
			AccessKeyID           string            `json:"accessKeyID"`
			SecretAccessKey       string            `json:"secretAccessKey"`
			RepositoryPathPattern string            `json:"repositoryPathPattern"`
			GitCredentials        map[string]string `json:"gitCredentials"`
		}{
			Region:                "us-west-1",
			AccessKeyID:           *awsAccessKeyID,
			SecretAccessKey:       *awsSecretAccessKey,
			RepositoryPathPattern: "aws/{name}",
			GitCredentials: map[string]string{
				"username": *awsCodeCommitUsername,
				"password": *awsCodeCommitPassword,
			},
		}),
	})
	// The repo-updater might not be up yet, but it will eventually catch up for the external
	// service we just added, thus it is OK to ignore this transient error.
	if err != nil && !strings.Contains(err.Error(), "/sync-external-service") {
		t.Fatal(err)
	}
	defer func() {
		err := client.DeleteExternalService(esID, false)
		if err != nil {
			t.Fatal(err)
		}
	}()

	const repoName = "aws/test"
	err = client.WaitForReposToBeCloned(repoName)
	if err != nil {
		t.Fatal(err)
	}

	blob, err := client.GitBlob(repoName, "master", "README")
	if err != nil {
		t.Fatal(err)
	}

	wantBlob := "README\n\nchange"
	if diff := cmp.Diff(wantBlob, blob); diff != "" {
		t.Fatalf("Blob mismatch (-want +got):\n%s", diff)
	}
}

func TestExternalService_BitbucketServer(t *testing.T) {
	if len(*bbsURL) == 0 || len(*bbsToken) == 0 || len(*bbsUsername) == 0 {
		t.Skip("Environment variable BITBUCKET_SERVER_URL, BITBUCKET_SERVER_TOKEN, or BITBUCKET_SERVER_USERNAME is not set")
	}

	// Set up external service
	esID, err := client.AddExternalService(gqltestutil.AddExternalServiceInput{
		Kind:        extsvc.KindBitbucketServer,
		DisplayName: "gqltest-bitbucket-server",
		Config: mustMarshalJSONString(struct {
			URL                   string   `json:"url"`
			Token                 string   `json:"token"`
			Username              string   `json:"username"`
			Repos                 []string `json:"repos"`
			RepositoryPathPattern string   `json:"repositoryPathPattern"`
		}{
			URL:                   *bbsURL,
			Token:                 *bbsToken,
			Username:              *bbsUsername,
			Repos:                 []string{"SOURCEGRAPH/jsonrpc2"},
			RepositoryPathPattern: "bbs/{projectKey}/{repositorySlug}",
		}),
	})
	// The repo-updater might not be up yet, but it will eventually catch up for the external
	// service we just added, thus it is OK to ignore this transient error.
	if err != nil && !strings.Contains(err.Error(), "/sync-external-service") {
		t.Fatal(err)
	}
	defer func() {
		err := client.DeleteExternalService(esID, false)
		if err != nil {
			t.Fatal(err)
		}
	}()

	const repoName = "bbs/SOURCEGRAPH/jsonrpc2"
	err = client.WaitForReposToBeCloned(repoName)
	if err != nil {
		t.Fatal(err)
	}

	blob, err := client.GitBlob(repoName, "master", ".travis.yml")
	if err != nil {
		t.Fatal(err)
	}

	wantBlob := "language: go\ngo: \n - 1.x\n\nscript:\n - go test -race -v ./...\n"
	if diff := cmp.Diff(wantBlob, blob); diff != "" {
		t.Fatalf("Blob mismatch (-want +got):\n%s", diff)
	}
}

func TestExternalService_Perforce(t *testing.T) {
	checkPerforceEnvironment(t)
	createPerforceExternalService(t)

	const repoName = "perforce/test-perms"
	err := client.WaitForReposToBeCloned(repoName)
	if err != nil {
		t.Fatal(err)
	}

	blob, err := client.GitBlob(repoName, "master", "README.md")
	if err != nil {
		t.Fatal(err)
	}

	wantBlob := `This depot is used to test user and group permissions.
`
	if diff := cmp.Diff(wantBlob, blob); diff != "" {
		t.Fatalf("Blob mismatch (-want +got):\n%s", diff)
	}
}

func checkPerforceEnvironment(t *testing.T) {
	// context: https://sourcegraph.slack.com/archives/C07KZF47K/p1658178309055259
	// But it seems that there is still an issue with P4 and they're currently timing out.
	// cc @mollylogue
	t.Skip("Currently broken")

	if len(*perforcePort) == 0 || len(*perforceUser) == 0 || len(*perforcePassword) == 0 {
		t.Skip("Environment variables PERFORCE_PORT, PERFORCE_USER or PERFORCE_PASSWORD are not set")
	}
}

func createPerforceExternalService(t *testing.T) {
	t.Helper()

	type Authorization = struct {
		SubRepoPermissions bool `json:"subRepoPermissions"`
	}

	// Set up external service
	esID, err := client.AddExternalService(gqltestutil.AddExternalServiceInput{
		Kind:        extsvc.KindPerforce,
		DisplayName: "gqltest-perforce-server",
		Config: mustMarshalJSONString(struct {
			P4Port                string        `json:"p4.port"`
			P4User                string        `json:"p4.user"`
			P4Password            string        `json:"p4.passwd"`
			Depots                []string      `json:"depots"`
			RepositoryPathPattern string        `json:"repositoryPathPattern"`
			Authorization         Authorization `json:"authorization"`
		}{
			P4Port:                *perforcePort,
			P4User:                *perforceUser,
			P4Password:            *perforcePassword,
			Depots:                []string{"//test-perms/"},
			RepositoryPathPattern: "perforce/{depot}",
			Authorization: Authorization{
				SubRepoPermissions: true,
			},
		}),
	})

	// The repo-updater might not be up yet but it will eventually catch up for the
	// external service we just added, thus it is OK to ignore this transient error.
	if err != nil && !strings.Contains(err.Error(), "/sync-external-service") {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		err := client.DeleteExternalService(esID, true)
		if err != nil {
			t.Fatal(err)
		}
	})
}

func TestExternalService_AsyncDeletion(t *testing.T) {
	if len(*bbsURL) == 0 || len(*bbsToken) == 0 || len(*bbsUsername) == 0 {
		t.Skip("Environment variable BITBUCKET_SERVER_URL, BITBUCKET_SERVER_TOKEN, or BITBUCKET_SERVER_USERNAME is not set")
	}

	// Set up external service
	esID, err := client.AddExternalService(gqltestutil.AddExternalServiceInput{
		Kind:        extsvc.KindBitbucketServer,
		DisplayName: "gqltest-bitbucket-server",
		Config: mustMarshalJSONString(struct {
			URL                   string   `json:"url"`
			Token                 string   `json:"token"`
			Username              string   `json:"username"`
			Repos                 []string `json:"repos"`
			RepositoryPathPattern string   `json:"repositoryPathPattern"`
		}{
			URL:                   *bbsURL,
			Token:                 *bbsToken,
			Username:              *bbsUsername,
			Repos:                 []string{"SOURCEGRAPH/jsonrpc2"},
			RepositoryPathPattern: "bbs/{projectKey}/{repositorySlug}",
		}),
	})
	// The repo-updater might not be up yet, but it will eventually catch up for the external
	// service we just added, thus it is OK to ignore this transient error.
	if err != nil && !strings.Contains(err.Error(), "/sync-external-service") {
		t.Fatal(err)
	}
	err = client.DeleteExternalService(esID, true)
	if err != nil {
		t.Fatal(err)
	}

	// This call should return not found error. Retrying for 5 seconds to wait for async deletion to finish
	err = gqltestutil.Retry(5*time.Second, func() error {
		_, err = client.UpdateExternalService(gqltestutil.UpdateExternalServiceInput{ID: esID})
		if err == nil {
			return gqltestutil.ErrContinueRetry
		}
		return err
	})
	if err == nil || err == gqltestutil.ErrContinueRetry {
		t.Fatal("Deleted service should not be found")
	}
	if !strings.Contains(err.Error(), "external service not found") {
		t.Fatalf("Not found error should be returned, got: %s", err.Error())
	}
}
