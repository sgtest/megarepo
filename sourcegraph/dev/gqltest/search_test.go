// +build gqltest

package main

import (
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/gqltestutil"
)

func TestSearch(t *testing.T) {
	if len(*githubToken) == 0 {
		t.Skip("Environment variable GITHUB_TOKEN is not set")
	}

	// Set up external service
	esID, err := client.AddExternalService(gqltestutil.AddExternalServiceInput{
		Kind:        extsvc.KindGitHub,
		DisplayName: "gqltest-github-search",
		Config: mustMarshalJSONString(struct {
			URL   string   `json:"url"`
			Token string   `json:"token"`
			Repos []string `json:"repos"`
		}{
			URL:   "http://github.com",
			Token: *githubToken,
			Repos: []string{
				"sourcegraph/java-langserver",
				"gorilla/mux",
				"gorilla/securecookie",
				"sourcegraph/jsonrpc2",
				"sourcegraph/go-diff",
				"sourcegraph/appdash",
				"sourcegraph/sourcegraph-typescript",
				"sourcegraph-testing/automation-e2e-test",
				"sourcegraph/e2e-test-private-repository",
			},
		}),
	})
	if err != nil {
		t.Fatal(err)
	}
	defer func() {
		err := client.DeleteExternalService(esID)
		if err != nil {
			t.Fatal(err)
		}
	}()

	err = client.WaitForReposToBeCloned(
		"github.com/sourcegraph/java-langserver",
		"github.com/gorilla/mux",
		"github.com/gorilla/securecookie",
		"github.com/sourcegraph/jsonrpc2",
		"github.com/sourcegraph/go-diff",
		"github.com/sourcegraph/appdash",
		"github.com/sourcegraph/sourcegraph-typescript",
		"github.com/sourcegraph-testing/automation-e2e-test",
		"github.com/sourcegraph/e2e-test-private-repository",
	)
	if err != nil {
		t.Fatal(err)
	}

	t.Run("visibility", func(t *testing.T) {
		tests := []struct {
			query       string
			wantMissing []string
		}{
			{
				query:       "type:repo visibility:private",
				wantMissing: []string{},
			},
			{
				query:       "type:repo visibility:public",
				wantMissing: []string{"github.com/sourcegraph/e2e-test-private-repository"},
			},
			{
				query:       "type:repo visibility:any",
				wantMissing: []string{},
			},
		}
		for _, test := range tests {
			t.Run(test.query, func(t *testing.T) {
				results, err := client.SearchRepositories(test.query)
				if err != nil {
					t.Fatal(err)
				}
				missing := results.Exists("github.com/sourcegraph/e2e-test-private-repository")
				if diff := cmp.Diff(test.wantMissing, missing); diff != "" {
					t.Fatalf("Missing mismatch (-want +got):\n%s", diff)
				}
			})
		}
	})

	t.Run("execute search with search parameters", func(t *testing.T) {
		results, err := client.SearchFiles("repo:^github.com/sourcegraph/go-diff$ type:file file:.go -file:.md")
		if err != nil {
			t.Fatal(err)
		}

		// Make sure only got .go files and no .md files
		for _, r := range results {
			if !strings.HasSuffix(r.File.Name, ".go") {
				t.Fatalf("Found file name does not end with .go: %s", r.File.Name)
			}
		}
	})

	t.Run("multiple revisions per repository", func(t *testing.T) {
		results, err := client.SearchFiles("repo:sourcegraph/go-diff$@master:print-options:*refs/heads/ func NewHunksReader")
		if err != nil {
			t.Fatal(err)
		}

		wantExprs := map[string]struct{}{
			"master":        {},
			"print-options": {},

			// These next 2 branches are included because of the *refs/heads/ in the query.
			// If they are ever deleted from the actual live repository, replace them with
			// any other branches that still exist.
			"test-already-exist-pr": {},
			"bug-fix-wip":           {},
		}

		for _, r := range results {
			delete(wantExprs, r.RevSpec.Expr)
		}

		if len(wantExprs) > 0 {
			missing := make([]string, 0, len(wantExprs))
			for expr := range wantExprs {
				missing = append(missing, expr)
			}
			t.Fatalf("Missing exprs: %v", missing)
		}
	})

	t.Run("search statistics", func(t *testing.T) {
		err := client.OverwriteSettings(client.AuthenticatedUserID(), `{"experimentalFeatures":{"searchStats": true}}`)
		if err != nil {
			t.Fatal(err)
		}
		defer func() {
			err := client.OverwriteSettings(client.AuthenticatedUserID(), `{}`)
			if err != nil {
				t.Fatal(err)
			}
		}()

		var lastResult *gqltestutil.SearchStatsResult
		// Retry because the configuration update endpoint is eventually consistent
		err = gqltestutil.Retry(5*time.Second, func() error {
			// This is a substring that appears in the sourcegraph/go-diff repository.
			// It is OK if it starts to appear in other repositories, the test just
			// checks that it is found in at least 1 Go file.
			result, err := client.SearchStats("Incomplete-Lines")
			if err != nil {
				t.Fatal(err)
			}
			lastResult = result

			for _, lang := range result.Languages {
				if strings.EqualFold(lang.Name, "Go") {
					return nil
				}
			}

			return gqltestutil.ErrContinueRetry
		})
		if err != nil {
			t.Fatal(err, "lastResult:", lastResult)
		}
	})
}
