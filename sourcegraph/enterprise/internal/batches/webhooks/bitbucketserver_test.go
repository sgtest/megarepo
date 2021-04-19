package webhooks

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
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

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/sources"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/syncer"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/testing"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/httptestutil"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

// Run from integration_test.go
func testBitbucketWebhook(db *sql.DB, userID int32) func(*testing.T) {
	return func(t *testing.T) {
		now := timeutil.Now()
		clock := func() time.Time { return now }

		ctx := context.Background()

		rcache.SetupForTest(t)

		ct.TruncateTables(t, db, "changeset_events", "changesets")

		cf, save := httptestutil.NewGitHubRecorderFactory(t, *update, "bitbucket-webhooks")
		defer save()

		secret := "secret"
		repoStore := database.Repos(db)
		esStore := database.ExternalServices(db)
		bitbucketServerToken := os.Getenv("BITBUCKET_SERVER_TOKEN")
		if bitbucketServerToken == "" {
			bitbucketServerToken = "test-token"
		}
		extSvc := &types.ExternalService{
			Kind:        extsvc.KindBitbucketServer,
			DisplayName: "Bitbucket",
			Config: ct.MarshalJSON(t, &schema.BitbucketServerConnection{
				Url:   "https://bitbucket.sgdev.org",
				Token: bitbucketServerToken,
				Repos: []string{"SOUR/automation-testing"},
				Webhooks: &schema.Webhooks{
					Secret: secret,
				},
			}),
		}

		err := esStore.Upsert(ctx, extSvc)
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

		err = repoStore.Create(ctx, bitbucketRepo)
		if err != nil {
			t.Fatal(err)
		}

		s := store.NewWithClock(db, clock)
		sourcer := sources.NewSourcer(cf)

		spec := &btypes.BatchSpec{
			NamespaceUserID: userID,
			UserID:          userID,
		}
		if err := s.CreateBatchSpec(ctx, spec); err != nil {
			t.Fatal(err)
		}

		batchChange := &btypes.BatchChange{
			Name:             "Test batch change",
			Description:      "Testing THE WEBHOOKS",
			InitialApplierID: userID,
			NamespaceUserID:  userID,
			LastApplierID:    userID,
			LastAppliedAt:    clock(),
			BatchSpecID:      spec.ID,
		}

		err = s.CreateBatchChange(ctx, batchChange)
		if err != nil {
			t.Fatal(err)
		}

		changesets := []*btypes.Changeset{
			{
				RepoID:              bitbucketRepo.ID,
				ExternalID:          "69",
				ExternalServiceType: bitbucketRepo.ExternalRepo.ServiceType,
				BatchChanges:        []btypes.BatchChangeAssoc{{BatchChangeID: batchChange.ID}},
			},
			{
				RepoID:              bitbucketRepo.ID,
				ExternalID:          "19",
				ExternalServiceType: bitbucketRepo.ExternalRepo.ServiceType,
				BatchChanges:        []btypes.BatchChangeAssoc{{BatchChangeID: batchChange.ID}},
			},
		}

		// Set up mocks to prevent the diffstat computation from trying to
		// use a real gitserver, and so we can control what diff is used to
		// create the diffstat.
		state := ct.MockChangesetSyncState(&protocol.RepoInfo{
			Name: "repo",
			VCS:  protocol.VCSInfo{URL: "https://example.com/repo/"},
		})
		defer state.Unmock()

		for _, ch := range changesets {
			if err := s.CreateChangeset(ctx, ch); err != nil {
				t.Fatal(err)
			}
			src, err := sourcer.ForRepo(ctx, s, bitbucketRepo)
			if err != nil {
				t.Fatal(err)
			}
			err = syncer.SyncChangeset(ctx, s, src, bitbucketRepo, ch)
			if err != nil {
				t.Fatal(err)
			}
		}

		hook := NewBitbucketServerWebhook(s)

		fixtureFiles, err := filepath.Glob("testdata/fixtures/webhooks/bitbucketserver/*.json")
		if err != nil {
			t.Fatal(err)
		}

		for _, fixtureFile := range fixtureFiles {
			_, name := path.Split(fixtureFile)
			name = strings.TrimSuffix(name, ".json")
			t.Run(name, func(t *testing.T) {
				ct.TruncateTables(t, db, "changeset_events")

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

				have, _, err := s.ListChangesetEvents(ctx, store.ListChangesetEventsOpts{})
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
					cmpopts.IgnoreFields(btypes.ChangesetEvent{}, "CreatedAt"),
					cmpopts.IgnoreFields(btypes.ChangesetEvent{}, "UpdatedAt"),
				}
				if diff := cmp.Diff(tc.ChangesetEvents, have, opts...); diff != "" {
					t.Error(diff)
				}

			})
		}
	}
}
