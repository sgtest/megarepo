package campaigns

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"flag"
	"io/ioutil"
	"net/http"
	"net/http/httptest"
	"os"
	"path"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/testing"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/httptestutil"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/schema"
)

var update = flag.Bool("update", false, "update testdata")

// Run from integration_test.go
func testGitHubWebhook(db *sql.DB, userID int32) func(*testing.T) {
	return func(t *testing.T) {
		now := time.Now().UTC().Truncate(time.Microsecond)
		clock := func() time.Time { return now }

		ctx := context.Background()

		rcache.SetupForTest(t)

		truncateTables(t, db, "changeset_events", "changesets")

		cf, save := httptestutil.NewGitHubRecorderFactory(t, *update, "github-webhooks")
		defer save()

		secret := "secret"
		repoStore := repos.NewDBStore(db, sql.TxOptions{})
		extSvc := &repos.ExternalService{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GitHub",
			Config: marshalJSON(t, &schema.GitHubConnection{
				Url:      "https://github.com",
				Token:    os.Getenv("GITHUB_TOKEN"),
				Repos:    []string{"sourcegraph/sourcegraph"},
				Webhooks: []*schema.GitHubWebhook{{Org: "sourcegraph", Secret: secret}},
			}),
		}

		err := repoStore.UpsertExternalServices(ctx, extSvc)
		if err != nil {
			t.Fatal(t)
		}

		githubSrc, err := repos.NewGithubSource(extSvc, cf)
		if err != nil {
			t.Fatal(t)
		}

		githubRepo, err := githubSrc.GetRepo(ctx, "sourcegraph/sourcegraph")
		if err != nil {
			t.Fatal(err)
		}

		err = repoStore.UpsertRepos(ctx, githubRepo)
		if err != nil {
			t.Fatal(err)
		}

		store := NewStoreWithClock(db, clock)

		spec := &campaigns.CampaignSpec{
			NamespaceUserID: userID,
			UserID:          userID,
		}
		if err := store.CreateCampaignSpec(ctx, spec); err != nil {
			t.Fatal(err)
		}

		campaign := &campaigns.Campaign{
			Name:             "Test campaign",
			Description:      "Testing THE WEBHOOKS",
			InitialApplierID: userID,
			NamespaceUserID:  userID,
			LastApplierID:    userID,
			LastAppliedAt:    clock(),
			CampaignSpecID:   spec.ID,
		}

		err = store.CreateCampaign(ctx, campaign)
		if err != nil {
			t.Fatal(err)
		}

		// NOTE: Your sample payload should apply to a PR with the number matching below
		changeset := &campaigns.Changeset{
			RepoID:              githubRepo.ID,
			ExternalID:          "10156",
			ExternalServiceType: githubRepo.ExternalRepo.ServiceType,
			CampaignIDs:         []int64{campaign.ID},
		}

		err = store.CreateChangeset(ctx, changeset)
		if err != nil {
			t.Fatal(err)
		}

		// Set up mocks to prevent the diffstat computation from trying to
		// use a real gitserver, and so we can control what diff is used to
		// create the diffstat.
		state := ct.MockChangesetSyncState(&protocol.RepoInfo{
			Name: "repo",
			VCS:  protocol.VCSInfo{URL: "https://example.com/repo/"},
		})
		defer state.Unmock()

		err = SyncChangesets(ctx, repoStore, store, cf, changeset)
		if err != nil {
			t.Fatal(err)
		}

		hook := NewGitHubWebhook(store, repoStore, clock)

		fixtureFiles, err := filepath.Glob("testdata/fixtures/webhooks/github/*.json")
		if err != nil {
			t.Fatal(err)
		}

		for _, fixtureFile := range fixtureFiles {
			_, name := path.Split(fixtureFile)
			name = strings.TrimSuffix(name, ".json")
			t.Run(name, func(t *testing.T) {
				truncateTables(t, db, "changeset_events")

				tc := loadWebhookTestCase(t, fixtureFile)

				// Send all events twice to ensure we are idempotent
				for i := 0; i < 2; i++ {
					for _, event := range tc.Payloads {
						u := extsvc.WebhookURL(extsvc.TypeGitHub, extSvc.ID, "https://example.com/")

						req, err := http.NewRequest("POST", u, bytes.NewReader(event.Data))
						if err != nil {
							t.Fatal(err)
						}
						req.Header.Set("X-Github-Event", event.PayloadType)
						req.Header.Set("X-Hub-Signature", sign(t, event.Data, []byte(secret)))

						rec := httptest.NewRecorder()
						hook.ServeHTTP(rec, req)
						resp := rec.Result()

						if resp.StatusCode != http.StatusOK {
							t.Fatalf("Non 200 code: %v", resp.StatusCode)
						}
					}
				}

				have, _, err := store.ListChangesetEvents(ctx, ListChangesetEventsOpts{Limit: -1})
				if err != nil {
					t.Fatal(err)
				}

				// Overwrite and format test case
				if *update {
					tc.ChangesetEvents = have
					data, err := json.MarshalIndent(tc, "  ", "  ")
					if err != nil {
						t.Fatal(err)
					}
					err = ioutil.WriteFile(fixtureFile, data, 0666)
					if err != nil {
						t.Fatal(err)
					}
				}

				opts := []cmp.Option{
					cmpopts.IgnoreFields(campaigns.ChangesetEvent{}, "CreatedAt"),
					cmpopts.IgnoreFields(campaigns.ChangesetEvent{}, "UpdatedAt"),
				}
				if diff := cmp.Diff(tc.ChangesetEvents, have, opts...); diff != "" {
					t.Error(diff)
				}

			})
		}
	}
}

// Run from integration_test.go
func testBitbucketWebhook(db *sql.DB, userID int32) func(*testing.T) {
	return func(t *testing.T) {
		now := time.Now().UTC().Truncate(time.Microsecond)
		clock := func() time.Time { return now }

		ctx := context.Background()

		rcache.SetupForTest(t)

		truncateTables(t, db, "changeset_events", "changesets")

		cf, save := httptestutil.NewGitHubRecorderFactory(t, *update, "bitbucket-webhooks")
		defer save()

		secret := "secret"
		repoStore := repos.NewDBStore(db, sql.TxOptions{})
		extSvc := &repos.ExternalService{
			Kind:        extsvc.KindBitbucketServer,
			DisplayName: "Bitbucket",
			Config: marshalJSON(t, &schema.BitbucketServerConnection{
				Url:   "https://bitbucket.sgdev.org",
				Token: os.Getenv("BITBUCKET_SERVER_TOKEN"),
				Repos: []string{"SOUR/automation-testing"},
				Webhooks: &schema.Webhooks{
					Secret: secret,
				},
			}),
		}

		err := repoStore.UpsertExternalServices(ctx, extSvc)
		if err != nil {
			t.Fatal(t)
		}

		bitbucketSource, err := repos.NewBitbucketServerSource(extSvc, cf)
		if err != nil {
			t.Fatal(t)
		}

		bitbucketRepo, err := getSingleRepo(ctx, bitbucketSource, "bitbucket.sgdev.org/SOUR/automation-testing")
		if err != nil {
			t.Fatal(err)
		}

		if bitbucketRepo == nil {
			t.Fatal("repo not found")
		}

		err = repoStore.UpsertRepos(ctx, bitbucketRepo)
		if err != nil {
			t.Fatal(err)
		}

		store := NewStoreWithClock(db, clock)

		spec := &campaigns.CampaignSpec{
			NamespaceUserID: userID,
			UserID:          userID,
		}
		if err := store.CreateCampaignSpec(ctx, spec); err != nil {
			t.Fatal(err)
		}

		campaign := &campaigns.Campaign{
			Name:             "Test campaign",
			Description:      "Testing THE WEBHOOKS",
			InitialApplierID: userID,
			NamespaceUserID:  userID,
			LastApplierID:    userID,
			LastAppliedAt:    clock(),
			CampaignSpecID:   spec.ID,
		}

		err = store.CreateCampaign(ctx, campaign)
		if err != nil {
			t.Fatal(err)
		}

		changesets := []*campaigns.Changeset{
			{
				RepoID:              bitbucketRepo.ID,
				ExternalID:          "69",
				ExternalServiceType: bitbucketRepo.ExternalRepo.ServiceType,
				CampaignIDs:         []int64{campaign.ID},
			},
			{
				RepoID:              bitbucketRepo.ID,
				ExternalID:          "19",
				ExternalServiceType: bitbucketRepo.ExternalRepo.ServiceType,
				CampaignIDs:         []int64{campaign.ID},
			},
		}

		for _, ch := range changesets {
			if err = store.CreateChangeset(ctx, ch); err != nil {
				t.Fatal(err)
			}
		}

		// Set up mocks to prevent the diffstat computation from trying to
		// use a real gitserver, and so we can control what diff is used to
		// create the diffstat.
		state := ct.MockChangesetSyncState(&protocol.RepoInfo{
			Name: "repo",
			VCS:  protocol.VCSInfo{URL: "https://example.com/repo/"},
		})
		defer state.Unmock()

		err = SyncChangesets(ctx, repoStore, store, cf, changesets...)
		if err != nil {
			t.Fatal(err)
		}

		hook := NewBitbucketServerWebhook(store, repoStore, clock, "testhook")

		fixtureFiles, err := filepath.Glob("testdata/fixtures/webhooks/bitbucketserver/*.json")
		if err != nil {
			t.Fatal(err)
		}

		for _, fixtureFile := range fixtureFiles {
			_, name := path.Split(fixtureFile)
			name = strings.TrimSuffix(name, ".json")
			t.Run(name, func(t *testing.T) {
				truncateTables(t, db, "changeset_events")

				tc := loadWebhookTestCase(t, fixtureFile)

				// Send all events twice to ensure we are idempotent
				for i := 0; i < 2; i++ {
					for _, event := range tc.Payloads {
						u := extsvc.WebhookURL(extsvc.TypeBitbucketServer, extSvc.ID, "https://example.com/")

						req, err := http.NewRequest("POST", u, bytes.NewReader(event.Data))
						if err != nil {
							t.Fatal(err)
						}
						req.Header.Set("X-Event-Key", event.PayloadType)
						req.Header.Set("X-Hub-Signature", sign(t, event.Data, []byte(secret)))

						rec := httptest.NewRecorder()
						hook.ServeHTTP(rec, req)
						resp := rec.Result()

						if resp.StatusCode != http.StatusOK {
							t.Fatalf("Non 200 code: %v", resp.StatusCode)
						}
					}
				}

				have, _, err := store.ListChangesetEvents(ctx, ListChangesetEventsOpts{Limit: -1})
				if err != nil {
					t.Fatal(err)
				}

				// Overwrite and format test case
				if *update {
					tc.ChangesetEvents = have
					data, err := json.MarshalIndent(tc, "  ", "  ")
					if err != nil {
						t.Fatal(err)
					}
					err = ioutil.WriteFile(fixtureFile, data, 0666)
					if err != nil {
						t.Fatal(err)
					}
				}

				opts := []cmp.Option{
					cmpopts.IgnoreFields(campaigns.ChangesetEvent{}, "CreatedAt"),
					cmpopts.IgnoreFields(campaigns.ChangesetEvent{}, "UpdatedAt"),
				}
				if diff := cmp.Diff(tc.ChangesetEvents, have, opts...); diff != "" {
					t.Error(diff)
				}

			})
		}
	}
}

func getSingleRepo(ctx context.Context, bitbucketSource *repos.BitbucketServerSource, name string) (*repos.Repo, error) {
	repoChan := make(chan repos.SourceResult)
	go func() {
		bitbucketSource.ListRepos(ctx, repoChan)
		close(repoChan)
	}()

	var bitbucketRepo *repos.Repo
	for result := range repoChan {
		if result.Err != nil {
			return nil, result.Err
		}
		if result.Repo == nil {
			continue
		}
		if result.Repo.Name == name {
			bitbucketRepo = result.Repo
		}
	}

	return bitbucketRepo, nil
}

type webhookTestCase struct {
	Payloads []struct {
		PayloadType string          `json:"payload_type"`
		Data        json.RawMessage `json:"data"`
	} `json:"payloads"`
	ChangesetEvents []*campaigns.ChangesetEvent `json:"changeset_events"`
}

func loadWebhookTestCase(t testing.TB, path string) webhookTestCase {
	t.Helper()

	bs, err := ioutil.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}

	var tc webhookTestCase
	if err := json.Unmarshal(bs, &tc); err != nil {
		t.Fatal(err)
	}
	for i, ev := range tc.ChangesetEvents {
		meta, err := campaigns.NewChangesetEventMetadata(ev.Kind)
		if err != nil {
			t.Fatal(err)
		}
		raw, err := json.Marshal(ev.Metadata)
		if err != nil {
			t.Fatal(err)
		}
		err = json.Unmarshal(raw, &meta)
		if err != nil {
			t.Fatal(err)
		}
		tc.ChangesetEvents[i].Metadata = meta
	}

	return tc
}

func sign(t *testing.T, message, secret []byte) string {
	t.Helper()

	mac := hmac.New(sha256.New, secret)

	_, err := mac.Write(message)
	if err != nil {
		t.Fatalf("writing hmac message failed: %s", err)
	}

	return "sha256=" + hex.EncodeToString(mac.Sum(nil))
}

func marshalJSON(t testing.TB, v interface{}) string {
	t.Helper()

	bs, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}

	return string(bs)
}
