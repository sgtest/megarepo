package processor

import (
	"bytes"
	"context"
	"database/sql"
	"fmt"
	"io"
	"net/http"
	"testing"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/global"
	stesting "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/sources/testing"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	bt "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/testing"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/webhooks"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func mockDoer(req *http.Request) (*http.Response, error) {
	return &http.Response{
		StatusCode: http.StatusOK,
		Body: io.NopCloser(bytes.NewReader([]byte(fmt.Sprintf(
			// The actual changeset returned by the mock client is not important,
			// as long as it satisfies the type for webhooks.gqlChangesetResponse
			`{"data": {"node": {"id": "%s","externalID": "%s","batchChanges": {"nodes": [{"id": "%s"}]},"repository": {"id": "%s","name": "github.com/test/test"},"createdAt": "2023-02-25T00:53:50Z","updatedAt": "2023-02-25T00:53:50Z","title": "%s","body": "%s","author": {"name": "%s", "email": "%s"},"state": "%s","labels": [],"externalURL": {"url": "%s"},"forkNamespace": null,"reviewState": "%s","checkState": null,"error": null,"syncerError": null,"forkName": null,"ownedByBatchChange": null}}}`,
			"123",
			"123",
			"123",
			"123",
			"title",
			"body",
			"author",
			"email",
			"OPEN",
			"some-url",
			"PENDING",
		)))),
	}, nil
}

func TestBulkProcessor(t *testing.T) {
	t.Parallel()

	logger := logtest.Scoped(t)

	orig := httpcli.InternalDoer

	httpcli.InternalDoer = httpcli.DoerFunc(mockDoer)
	t.Cleanup(func() { httpcli.InternalDoer = orig })

	ctx := context.Background()
	sqlDB := dbtest.NewDB(logger, t)
	tx := dbtest.NewTx(t, sqlDB)
	db := database.NewDB(logger, sqlDB)
	bstore := store.New(database.NewDBWith(logger, basestore.NewWithHandle(basestore.NewHandleWithTx(tx, sql.TxOptions{}))), &observation.TestContext, nil)
	wstore := database.OutboundWebhookJobsWith(bstore, nil)

	user := bt.CreateTestUser(t, db, true)
	repo, _ := bt.CreateTestRepo(t, ctx, db)
	bt.CreateTestSiteCredential(t, bstore, repo)
	batchSpec := bt.CreateBatchSpec(t, ctx, bstore, "test-bulk", user.ID, 0)
	batchChange := bt.CreateBatchChange(t, ctx, bstore, "test-bulk", user.ID, batchSpec.ID)
	changesetSpec := bt.CreateChangesetSpec(t, ctx, bstore, bt.TestSpecOpts{
		User:      user.ID,
		Repo:      repo.ID,
		BatchSpec: batchSpec.ID,
		Typ:       btypes.ChangesetSpecTypeBranch,
		HeadRef:   "main",
	})
	changeset := bt.CreateChangeset(t, ctx, bstore, bt.TestChangesetOpts{
		Repo:                repo.ID,
		BatchChanges:        []types.BatchChangeAssoc{{BatchChangeID: batchChange.ID}},
		Metadata:            &github.PullRequest{},
		ExternalServiceType: extsvc.TypeGitHub,
		CurrentSpec:         changesetSpec.ID,
	})

	t.Run("Unknown job type", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}
		job := &types.ChangesetJob{JobType: types.ChangesetJobType("UNKNOWN"), UserID: user.ID}
		afterDone, err := bp.Process(ctx, job)
		if err == nil || err.Error() != `invalid job type "UNKNOWN"` {
			t.Fatalf("unexpected error returned %s", err)
		}
		if afterDone != nil {
			t.Fatal("unexpected non-nil afterDone")
		}
	})

	t.Run("changeset is processing", func(t *testing.T) {
		processingChangeset := bt.CreateChangeset(t, ctx, bstore, bt.TestChangesetOpts{
			Repo:                repo.ID,
			BatchChanges:        []types.BatchChangeAssoc{{BatchChangeID: batchChange.ID}},
			Metadata:            &github.PullRequest{},
			ExternalServiceType: extsvc.TypeGitHub,
			CurrentSpec:         changesetSpec.ID,
			ReconcilerState:     btypes.ReconcilerStateProcessing,
		})

		job := &types.ChangesetJob{
			// JobType doesn't matter but we need one for database validation
			JobType:     types.ChangesetJobTypeComment,
			ChangesetID: processingChangeset.ID,
			UserID:      user.ID,
		}
		if err := bstore.CreateChangesetJob(ctx, job); err != nil {
			t.Fatal(err)
		}

		bp := &bulkProcessor{tx: bstore, logger: logtest.Scoped(t)}
		afterDone, err := bp.Process(ctx, job)
		if err != changesetIsProcessingErr {
			t.Fatalf("unexpected error. want=%s, got=%s", changesetIsProcessingErr, err)
		}
		if afterDone != nil {
			t.Fatal("unexpected non-nil afterDone")
		}
	})

	t.Run("Comment job", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}
		job := &types.ChangesetJob{
			JobType:     types.ChangesetJobTypeComment,
			ChangesetID: changeset.ID,
			UserID:      user.ID,
			Payload:     &btypes.ChangesetJobCommentPayload{},
		}
		if err := bstore.CreateChangesetJob(ctx, job); err != nil {
			t.Fatal(err)
		}
		afterDone, err := bp.Process(ctx, job)
		if err != nil {
			t.Fatal(err)
		}
		if !fake.CreateCommentCalled {
			t.Fatal("expected CreateComment to be called but wasn't")
		}
		if afterDone != nil {
			t.Fatal("unexpected non-nil afterDone")
		}
	})

	t.Run("Detach job", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}
		job := &types.ChangesetJob{
			JobType:       types.ChangesetJobTypeDetach,
			ChangesetID:   changeset.ID,
			UserID:        user.ID,
			BatchChangeID: batchChange.ID,
			Payload:       &btypes.ChangesetJobDetachPayload{},
		}

		afterDone, err := bp.Process(ctx, job)
		if err != nil {
			t.Fatal(err)
		}
		ch, err := bstore.GetChangesetByID(ctx, changeset.ID)
		if err != nil {
			t.Fatal(err)
		}
		if afterDone != nil {
			t.Fatal("unexpected non-nil afterDone")
		}
		if len(ch.BatchChanges) != 1 {
			t.Fatalf("invalid batch changes associated, expected one, got=%+v", ch.BatchChanges)
		}
		if !ch.BatchChanges[0].Detach {
			t.Fatal("not marked as to be detached")
		}
		if ch.ReconcilerState != btypes.ReconcilerStateQueued {
			t.Fatalf("invalid reconciler state, got=%q", ch.ReconcilerState)
		}
	})

	t.Run("Reenqueue job", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}
		job := &types.ChangesetJob{
			JobType:     types.ChangesetJobTypeReenqueue,
			ChangesetID: changeset.ID,
			UserID:      user.ID,
			Payload:     &btypes.ChangesetJobReenqueuePayload{},
		}
		changeset.ReconcilerState = btypes.ReconcilerStateFailed
		if err := bstore.UpdateChangeset(ctx, changeset); err != nil {
			t.Fatal(err)
		}
		afterDone, err := bp.Process(ctx, job)
		if err != nil {
			t.Fatal(err)
		}
		if afterDone != nil {
			t.Fatal("unexpected non-nil afterDone")
		}
		changeset, err = bstore.GetChangesetByID(ctx, changeset.ID)
		if err != nil {
			t.Fatal(err)
		}
		if have, want := changeset.ReconcilerState, btypes.ReconcilerStateQueued; have != want {
			t.Fatalf("unexpected reconciler state, have=%q want=%q", have, want)
		}
	})

	t.Run("Merge job", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}
		job := &types.ChangesetJob{
			JobType:     types.ChangesetJobTypeMerge,
			ChangesetID: changeset.ID,
			UserID:      user.ID,
			Payload:     &btypes.ChangesetJobMergePayload{},
		}
		afterDone, err := bp.Process(ctx, job)
		if err != nil {
			t.Fatal(err)
		}
		if !fake.MergeChangesetCalled {
			t.Fatal("expected MergeChangeset to be called but wasn't")
		}
		if afterDone == nil {
			t.Fatal("unexpected nil afterDone")
		}

		// Ensure that the appropriate webhook job will be created
		afterDone(bstore)
		webhook, err := wstore.GetLast(ctx)

		if err != nil {
			t.Fatalf("could not get latest webhook job: %s", err)
		}
		if webhook == nil {
			t.Fatalf("expected webhook job to be created")
		}
		if webhook.EventType != webhooks.ChangesetClose {
			t.Fatalf("wrong webhook job type. want=%s, have=%s", webhooks.ChangesetClose, webhook.EventType)
		}
	})

	t.Run("Close job", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{FakeMetadata: &github.PullRequest{}}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}
		job := &types.ChangesetJob{
			JobType:     types.ChangesetJobTypeClose,
			ChangesetID: changeset.ID,
			UserID:      user.ID,
			Payload:     &btypes.ChangesetJobClosePayload{},
		}
		afterDone, err := bp.Process(ctx, job)
		if err != nil {
			t.Fatal(err)
		}
		if !fake.CloseChangesetCalled {
			t.Fatal("expected CloseChangeset to be called but wasn't")
		}
		if afterDone == nil {
			t.Fatal("unexpected nil afterDone")
		}

		// Ensure that the appropriate webhook job will be created
		afterDone(bstore)
		webhook, err := wstore.GetLast(ctx)

		if err != nil {
			t.Fatalf("could not get latest webhook job: %s", err)
		}
		if webhook == nil {
			t.Fatalf("expected webhook job to be created")
		}
		if webhook.EventType != webhooks.ChangesetClose {
			t.Fatalf("wrong webhook job type. want=%s, have=%s", webhooks.ChangesetClose, webhook.EventType)
		}
	})

	t.Run("Publish job", func(t *testing.T) {
		fake := &stesting.FakeChangesetSource{FakeMetadata: &github.PullRequest{}}
		bp := &bulkProcessor{
			tx:      bstore,
			sourcer: stesting.NewFakeSourcer(nil, fake),
			logger:  logtest.Scoped(t),
		}

		t.Run("errors", func(t *testing.T) {
			for name, tc := range map[string]struct {
				spec          *bt.TestSpecOpts
				changeset     bt.TestChangesetOpts
				wantRetryable bool
			}{
				"imported changeset": {
					spec: nil,
					changeset: bt.TestChangesetOpts{
						Repo:             repo.ID,
						BatchChange:      batchChange.ID,
						CurrentSpec:      0,
						ReconcilerState:  btypes.ReconcilerStateCompleted,
						PublicationState: btypes.ChangesetPublicationStatePublished,
						ExternalState:    btypes.ChangesetExternalStateOpen,
					},
					wantRetryable: false,
				},
				"bogus changeset spec ID, dude": {
					spec: nil,
					changeset: bt.TestChangesetOpts{
						Repo:             repo.ID,
						BatchChange:      batchChange.ID,
						CurrentSpec:      -1,
						ReconcilerState:  btypes.ReconcilerStateCompleted,
						PublicationState: btypes.ChangesetPublicationStatePublished,
						ExternalState:    btypes.ChangesetExternalStateOpen,
					},
					wantRetryable: false,
				},
				"publication state set": {
					spec: &bt.TestSpecOpts{
						User:      user.ID,
						Repo:      repo.ID,
						BatchSpec: batchSpec.ID,
						HeadRef:   "main",
						Typ:       btypes.ChangesetSpecTypeBranch,
						Published: false,
					},
					changeset: bt.TestChangesetOpts{
						Repo:             repo.ID,
						BatchChange:      batchChange.ID,
						ReconcilerState:  btypes.ReconcilerStateCompleted,
						PublicationState: btypes.ChangesetPublicationStateUnpublished,
					},
					wantRetryable: false,
				},
			} {
				t.Run(name, func(t *testing.T) {
					var changesetSpec *btypes.ChangesetSpec
					if tc.spec != nil {
						changesetSpec = bt.CreateChangesetSpec(t, ctx, bstore, *tc.spec)
					}

					if changesetSpec != nil {
						tc.changeset.CurrentSpec = changesetSpec.ID
					}
					changeset := bt.CreateChangeset(t, ctx, bstore, tc.changeset)

					job := &types.ChangesetJob{
						JobType:       types.ChangesetJobTypePublish,
						BatchChangeID: batchChange.ID,
						ChangesetID:   changeset.ID,
						UserID:        user.ID,
						Payload: &types.ChangesetJobPublishPayload{
							Draft: false,
						},
					}

					afterDone, err := bp.Process(ctx, job)
					if err == nil {
						t.Errorf("unexpected nil error")
					}
					if tc.wantRetryable && errcode.IsNonRetryable(err) {
						t.Errorf("error is not retryable: %v", err)
					}
					if !tc.wantRetryable && !errcode.IsNonRetryable(err) {
						t.Errorf("error is retryable: %v", err)
					}
					// We don't expect any afterDone function to be returned
					// because the bulk operation just enqueues the changesets for
					// publishing via the reconciler and does not actually perform
					// the publishing itself.
					if afterDone != nil {
						t.Fatal("unexpected non-nil afterDone")
					}
				})
			}
		})

		t.Run("success", func(t *testing.T) {
			for _, reconcilerState := range []btypes.ReconcilerState{
				btypes.ReconcilerStateCompleted,
				btypes.ReconcilerStateErrored,
				btypes.ReconcilerStateFailed,
				btypes.ReconcilerStateQueued,
				btypes.ReconcilerStateScheduled,
			} {
				t.Run(string(reconcilerState), func(t *testing.T) {
					for name, draft := range map[string]bool{
						"draft":     true,
						"published": false,
					} {
						t.Run(name, func(t *testing.T) {
							changesetSpec := bt.CreateChangesetSpec(t, ctx, bstore, bt.TestSpecOpts{
								User:      user.ID,
								Repo:      repo.ID,
								BatchSpec: batchSpec.ID,
								HeadRef:   "main",
								Typ:       btypes.ChangesetSpecTypeBranch,
							})
							changeset := bt.CreateChangeset(t, ctx, bstore, bt.TestChangesetOpts{
								Repo:             repo.ID,
								BatchChange:      batchChange.ID,
								CurrentSpec:      changesetSpec.ID,
								ReconcilerState:  reconcilerState,
								PublicationState: btypes.ChangesetPublicationStateUnpublished,
							})

							job := &types.ChangesetJob{
								JobType:       types.ChangesetJobTypePublish,
								BatchChangeID: batchChange.ID,
								ChangesetID:   changeset.ID,
								UserID:        user.ID,
								Payload: &types.ChangesetJobPublishPayload{
									Draft: draft,
								},
							}

							afterDone, err := bp.Process(ctx, job)
							if err != nil {
								t.Errorf("unexpected error: %v", err)
							}
							// We don't expect any afterDone function to be returned
							// because the bulk operation just enqueues the changesets for
							// publishing via the reconciler and does not actually perform
							// the publishing itself.
							if afterDone != nil {
								t.Fatal("unexpected non-nil afterDone")
							}

							changeset, err = bstore.GetChangesetByID(ctx, changeset.ID)
							if err != nil {
								t.Fatal(err)
							}

							var want btypes.ChangesetUiPublicationState
							if draft {
								want = btypes.ChangesetUiPublicationStateDraft
							} else {
								want = btypes.ChangesetUiPublicationStatePublished
							}
							if have := changeset.UiPublicationState; have == nil || *have != want {
								t.Fatalf("unexpected UI publication state: have=%v want=%q", have, want)
							}

							if have, want := changeset.ReconcilerState, global.DefaultReconcilerEnqueueState(); have != want {
								t.Fatalf("unexpected reconciler state, have=%q want=%q", have, want)
							}
						})
					}
				})
			}
		})
	})
}
