package resolvers

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/resolvers/apitest"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/search"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/service"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/testing"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	batcheslib "github.com/sourcegraph/sourcegraph/lib/batches"
	"github.com/sourcegraph/sourcegraph/lib/batches/overridable"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestNullIDResilience(t *testing.T) {
	ct.MockRSAKeygen(t)

	logger := logtest.Scoped(t)

	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	sr := New(store.New(db, &observation.TestContext, nil))

	s, err := newSchema(db, sr)
	if err != nil {
		t.Fatal(err)
	}

	ctx := actor.WithInternalActor(context.Background())

	ids := []graphql.ID{
		marshalBatchChangeID(0),
		marshalChangesetID(0),
		marshalBatchSpecRandID(""),
		marshalChangesetSpecRandID(""),
		marshalBatchChangesCredentialID(0, false),
		marshalBatchChangesCredentialID(0, true),
		marshalBulkOperationID(""),
		marshalBatchSpecWorkspaceID(0),
	}

	for _, id := range ids {
		var response struct{ Node struct{ ID string } }

		query := `query($id: ID!) { node(id: $id) { id } }`
		if errs := apitest.Exec(ctx, t, s, map[string]any{"id": id}, &response, query); len(errs) > 0 {
			t.Errorf("GraphQL request failed: %#+v", errs[0])
		}

		if have, want := response.Node.ID, ""; have != want {
			t.Errorf("node has wrong ID. have=%q, want=%q", have, want)
		}
	}

	mutations := []string{
		fmt.Sprintf(`mutation { closeBatchChange(batchChange: %q) { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { deleteBatchChange(batchChange: %q) { alwaysNil } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { syncChangeset(changeset: %q) { alwaysNil } }`, marshalChangesetID(0)),
		fmt.Sprintf(`mutation { reenqueueChangeset(changeset: %q) { id } }`, marshalChangesetID(0)),
		fmt.Sprintf(`mutation { applyBatchChange(batchSpec: %q) { id } }`, marshalBatchSpecRandID("")),
		fmt.Sprintf(`mutation { createBatchChange(batchSpec: %q) { id } }`, marshalBatchSpecRandID("")),
		fmt.Sprintf(`mutation { moveBatchChange(batchChange: %q, newName: "foobar") { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { createBatchChangesCredential(externalServiceKind: GITHUB, externalServiceURL: "http://test", credential: "123123", user: %q) { id } }`, graphqlbackend.MarshalUserID(0)),
		fmt.Sprintf(`mutation { deleteBatchChangesCredential(batchChangesCredential: %q) { alwaysNil } }`, marshalBatchChangesCredentialID(0, false)),
		fmt.Sprintf(`mutation { deleteBatchChangesCredential(batchChangesCredential: %q) { alwaysNil } }`, marshalBatchChangesCredentialID(0, true)),
		fmt.Sprintf(`mutation { createChangesetComments(batchChange: %q, changesets: [], body: "test") { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { createChangesetComments(batchChange: %q, changesets: [%q], body: "test") { id } }`, marshalBatchChangeID(1), marshalChangesetID(0)),
		fmt.Sprintf(`mutation { reenqueueChangesets(batchChange: %q, changesets: []) { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { reenqueueChangesets(batchChange: %q, changesets: [%q]) { id } }`, marshalBatchChangeID(1), marshalChangesetID(0)),
		fmt.Sprintf(`mutation { mergeChangesets(batchChange: %q, changesets: []) { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { mergeChangesets(batchChange: %q, changesets: [%q]) { id } }`, marshalBatchChangeID(1), marshalChangesetID(0)),
		fmt.Sprintf(`mutation { closeChangesets(batchChange: %q, changesets: []) { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { closeChangesets(batchChange: %q, changesets: [%q]) { id } }`, marshalBatchChangeID(1), marshalChangesetID(0)),
		fmt.Sprintf(`mutation { publishChangesets(batchChange: %q, changesets: []) { id } }`, marshalBatchChangeID(0)),
		fmt.Sprintf(`mutation { publishChangesets(batchChange: %q, changesets: [%q]) { id } }`, marshalBatchChangeID(1), marshalChangesetID(0)),
		fmt.Sprintf(`mutation { executeBatchSpec(batchSpec: %q) { id } }`, marshalBatchSpecRandID("")),
		fmt.Sprintf(`mutation { cancelBatchSpecExecution(batchSpec: %q) { id } }`, marshalBatchSpecRandID("")),
		fmt.Sprintf(`mutation { replaceBatchSpecInput(previousSpec: %q, batchSpec: "name: testing") { id } }`, marshalBatchSpecRandID("")),
		fmt.Sprintf(`mutation { retryBatchSpecWorkspaceExecution(batchSpecWorkspaces: [%q]) { alwaysNil } }`, marshalBatchSpecWorkspaceID(0)),
		fmt.Sprintf(`mutation { retryBatchSpecExecution(batchSpec: %q) { id } }`, marshalBatchSpecRandID("")),
	}

	for _, m := range mutations {
		var response struct{}
		errs := apitest.Exec(ctx, t, s, nil, &response, m)
		if len(errs) == 0 {
			t.Errorf("expected errors but none returned (mutation: %q)", m)
		}
		if have, want := errs[0].Error(), fmt.Sprintf("graphql: %s", ErrIDIsZero{}); have != want {
			t.Errorf("wrong errors. have=%s, want=%s (mutation: %q)", have, want, m)
		}
	}
}

func TestCreateBatchSpec(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	user := ct.CreateTestUser(t, db, true)
	userID := user.ID

	cstore := store.New(db, &observation.TestContext, nil)
	repoStore := database.ReposWith(logger, cstore)
	esStore := database.ExternalServicesWith(logger, cstore)

	repo := newGitHubTestRepo("github.com/sourcegraph/create-batch-spec-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	// Create enough changeset specs to hit the licence check.
	changesetSpecs := make([]*btypes.ChangesetSpec, maxUnlicensedChangesets+1)
	for i := range changesetSpecs {
		changesetSpecs[i] = &btypes.ChangesetSpec{
			Spec: &batcheslib.ChangesetSpec{
				BaseRepository: string(graphqlbackend.MarshalRepositoryID(repo.ID)),
			},
			RepoID: repo.ID,
			UserID: userID,
		}
		if err := cstore.CreateChangesetSpec(ctx, changesetSpecs[i]); err != nil {
			t.Fatal(err)
		}
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)
	if err != nil {
		t.Fatal(err)
	}

	userAPIID := string(graphqlbackend.MarshalUserID(userID))
	rawSpec := ct.TestRawBatchSpec

	for name, tc := range map[string]struct {
		changesetSpecs []*btypes.ChangesetSpec
		hasLicenseFor  map[licensing.Feature]struct{}
		wantErr        bool
	}{
		"batch changes license, over the limit": {
			changesetSpecs: changesetSpecs,
			hasLicenseFor: map[licensing.Feature]struct{}{
				licensing.FeatureBatchChanges: {},
			},
			wantErr: false,
		},
		"campaigns license, over the limit": {
			changesetSpecs: changesetSpecs,
			hasLicenseFor: map[licensing.Feature]struct{}{
				licensing.FeatureCampaigns: {},
			},
			wantErr: false,
		},
		"no licence, but under the limit": {
			changesetSpecs: changesetSpecs[0:maxUnlicensedChangesets],
			hasLicenseFor:  map[licensing.Feature]struct{}{},
			wantErr:        false,
		},
		"no licence, over the limit": {
			changesetSpecs: changesetSpecs,
			hasLicenseFor:  map[licensing.Feature]struct{}{},
			wantErr:        true,
		},
	} {
		t.Run(name, func(t *testing.T) {
			oldMock := licensing.MockCheckFeature
			licensing.MockCheckFeature = func(feature licensing.Feature) error {
				if _, ok := tc.hasLicenseFor[feature]; !ok {
					return licensing.NewFeatureNotActivatedError("no batch changes for you!")
				}
				return nil
			}

			defer func() {
				licensing.MockCheckFeature = oldMock
			}()

			changesetSpecIDs := make([]graphql.ID, len(tc.changesetSpecs))
			for i, spec := range tc.changesetSpecs {
				changesetSpecIDs[i] = marshalChangesetSpecRandID(spec.RandID)
			}

			input := map[string]any{
				"namespace":      userAPIID,
				"batchSpec":      rawSpec,
				"changesetSpecs": changesetSpecIDs,
			}

			var response struct{ CreateBatchSpec apitest.BatchSpec }

			actorCtx := actor.WithActor(ctx, actor.FromUser(userID))
			errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateBatchSpec)
			if tc.wantErr {
				if errs == nil {
					t.Error("unexpected lack of errors")
				}
			} else {
				if errs != nil {
					t.Errorf("unexpected error(s): %+v", errs)
				}

				var unmarshaled any
				err = json.Unmarshal([]byte(rawSpec), &unmarshaled)
				if err != nil {
					t.Fatal(err)
				}
				have := response.CreateBatchSpec

				wantNodes := make([]apitest.ChangesetSpec, len(changesetSpecIDs))
				for i, id := range changesetSpecIDs {
					wantNodes[i] = apitest.ChangesetSpec{
						Typename: "VisibleChangesetSpec",
						ID:       string(id),
					}
				}

				applyUrl := fmt.Sprintf("/users/%s/batch-changes/apply/%s", user.Username, have.ID)
				want := apitest.BatchSpec{
					ID:            have.ID,
					CreatedAt:     have.CreatedAt,
					ExpiresAt:     have.ExpiresAt,
					OriginalInput: rawSpec,
					ParsedInput:   graphqlbackend.JSONValue{Value: unmarshaled},
					ApplyURL:      &applyUrl,
					Namespace:     apitest.UserOrg{ID: userAPIID, DatabaseID: userID, SiteAdmin: true},
					Creator:       &apitest.User{ID: userAPIID, DatabaseID: userID, SiteAdmin: true},
					ChangesetSpecs: apitest.ChangesetSpecConnection{
						Nodes: wantNodes,
					},
				}

				if diff := cmp.Diff(want, have); diff != "" {
					t.Fatalf("unexpected response (-want +got):\n%s", diff)
				}
			}
		})
	}
}

const mutationCreateBatchSpec = `
fragment u on User { id, databaseID, siteAdmin }
fragment o on Org  { id, name }

mutation($namespace: ID!, $batchSpec: String!, $changesetSpecs: [ID!]!){
  createBatchSpec(namespace: $namespace, batchSpec: $batchSpec, changesetSpecs: $changesetSpecs) {
    id
    originalInput
    parsedInput

    creator  { ...u }
    namespace {
      ... on User { ...u }
      ... on Org  { ...o }
    }

    applyURL

	changesetSpecs {
	  nodes {
		  __typename
		  ... on VisibleChangesetSpec {
			  id
		  }
	  }
	}

    createdAt
    expiresAt
  }
}
`

func TestCreateChangesetSpec(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := ct.CreateTestUser(t, db, true).ID

	cstore := store.New(db, &observation.TestContext, nil)
	repoStore := database.ReposWith(logger, cstore)
	esStore := database.ExternalServicesWith(logger, cstore)

	repo := newGitHubTestRepo("github.com/sourcegraph/create-changeset-spec-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	input := map[string]any{
		"changesetSpec": ct.NewRawChangesetSpecGitBranch(graphqlbackend.MarshalRepositoryID(repo.ID), "d34db33f"),
	}

	var response struct{ CreateChangesetSpec apitest.ChangesetSpec }

	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))
	apitest.MustExec(actorCtx, t, s, input, &response, mutationCreateChangesetSpec)

	have := response.CreateChangesetSpec

	want := apitest.ChangesetSpec{
		Typename:  "VisibleChangesetSpec",
		ID:        have.ID,
		ExpiresAt: have.ExpiresAt,
	}

	if diff := cmp.Diff(want, have); diff != "" {
		t.Fatalf("unexpected response (-want +got):\n%s", diff)
	}

	randID, err := unmarshalChangesetSpecID(graphql.ID(want.ID))
	if err != nil {
		t.Fatal(err)
	}

	cs, err := cstore.GetChangesetSpec(ctx, store.GetChangesetSpecOpts{RandID: randID})
	if err != nil {
		t.Fatal(err)
	}

	if have, want := cs.RepoID, repo.ID; have != want {
		t.Fatalf("wrong RepoID. want=%d, have=%d", want, have)
	}
}

const mutationCreateChangesetSpec = `
mutation($changesetSpec: String!){
  createChangesetSpec(changesetSpec: $changesetSpec) {
	__typename
	... on VisibleChangesetSpec {
		id
		expiresAt
	}
  }
}
`

func TestApplyBatchChange(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	// Ensure our site configuration doesn't have rollout windows so we get a
	// consistent initial state.
	ct.MockConfig(t, &conf.Unified{})

	userID := ct.CreateTestUser(t, db, true).ID

	now := timeutil.Now()
	clock := func() time.Time { return now }
	cstore := store.NewWithClock(db, &observation.TestContext, nil, clock)
	repoStore := database.ReposWith(logger, cstore)
	esStore := database.ExternalServicesWith(logger, cstore)

	repo := newGitHubTestRepo("github.com/sourcegraph/apply-batch-change-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	repoAPIID := graphqlbackend.MarshalRepositoryID(repo.ID)

	falsy := overridable.FromBoolOrString(false)
	batchSpec := &btypes.BatchSpec{
		RawSpec: ct.TestRawBatchSpec,
		Spec: &batcheslib.BatchSpec{
			Name:        "my-batch-change",
			Description: "My description",
			ChangesetTemplate: &batcheslib.ChangesetTemplate{
				Title:  "Hello there",
				Body:   "This is the body",
				Branch: "my-branch",
				Commit: batcheslib.ExpandedGitCommitDescription{
					Message: "Add hello world",
				},
				Published: &falsy,
			},
		},
		UserID:          userID,
		NamespaceUserID: userID,
	}
	if err := cstore.CreateBatchSpec(ctx, batchSpec); err != nil {
		t.Fatal(err)
	}

	changesetSpec := &btypes.ChangesetSpec{
		BatchSpecID: batchSpec.ID,
		Spec: &batcheslib.ChangesetSpec{
			BaseRepository: string(repoAPIID),
		},
		RepoID: repo.ID,
		UserID: userID,
	}
	if err := cstore.CreateChangesetSpec(ctx, changesetSpec); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	userAPIID := string(graphqlbackend.MarshalUserID(userID))
	input := map[string]any{
		"batchSpec": string(marshalBatchSpecRandID(batchSpec.RandID)),
	}

	var response struct{ ApplyBatchChange apitest.BatchChange }
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))
	apitest.MustExec(actorCtx, t, s, input, &response, mutationApplyBatchChange)

	apiUser := &apitest.User{
		ID:         userAPIID,
		DatabaseID: userID,
		SiteAdmin:  true,
	}

	have := response.ApplyBatchChange
	want := apitest.BatchChange{
		ID:          have.ID,
		Name:        batchSpec.Spec.Name,
		Description: batchSpec.Spec.Description,
		Namespace: apitest.UserOrg{
			ID:         userAPIID,
			DatabaseID: userID,
			SiteAdmin:  true,
		},
		Creator:       apiUser,
		LastApplier:   apiUser,
		LastAppliedAt: marshalDateTime(t, now),
		Changesets: apitest.ChangesetConnection{
			Nodes: []apitest.Changeset{
				{Typename: "ExternalChangeset", State: string(btypes.ChangesetStateProcessing)},
			},
			TotalCount: 1,
		},
	}

	if diff := cmp.Diff(want, have); diff != "" {
		t.Fatalf("unexpected response (-want +got):\n%s", diff)
	}

	// Now we execute it again and make sure we get the same batch change back
	apitest.MustExec(actorCtx, t, s, input, &response, mutationApplyBatchChange)
	have2 := response.ApplyBatchChange
	if diff := cmp.Diff(want, have2); diff != "" {
		t.Fatalf("unexpected response (-want +got):\n%s", diff)
	}

	// Execute it again with ensureBatchChange set to correct batch change's ID
	input["ensureBatchChange"] = have2.ID
	apitest.MustExec(actorCtx, t, s, input, &response, mutationApplyBatchChange)
	have3 := response.ApplyBatchChange
	if diff := cmp.Diff(want, have3); diff != "" {
		t.Fatalf("unexpected response (-want +got):\n%s", diff)
	}

	// Execute it again but ensureBatchChange set to wrong batch change ID
	batchChangeID, err := unmarshalBatchChangeID(graphql.ID(have3.ID))
	if err != nil {
		t.Fatal(err)
	}
	input["ensureBatchChange"] = marshalBatchChangeID(batchChangeID + 999)
	errs := apitest.Exec(actorCtx, t, s, input, &response, mutationApplyBatchChange)
	if len(errs) == 0 {
		t.Fatalf("expected errors, got none")
	}
}

const fragmentBatchChange = `
fragment u on User { id, databaseID, siteAdmin }
fragment o on Org  { id, name }
fragment batchChange on BatchChange {
	id, name, description
    creator           { ...u }
    lastApplier       { ...u }
    lastAppliedAt
    namespace {
        ... on User { ...u }
        ... on Org  { ...o }
    }

    changesets {
		nodes {
			__typename
			state
		}

		totalCount
    }
}
`

const mutationApplyBatchChange = `
mutation($batchSpec: ID!, $ensureBatchChange: ID, $publicationStates: [ChangesetSpecPublicationStateInput!]){
	applyBatchChange(batchSpec: $batchSpec, ensureBatchChange: $ensureBatchChange, publicationStates: $publicationStates) {
		...batchChange
	}
}
` + fragmentBatchChange

func TestCreateEmptyBatchChange(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	cstore := store.New(db, &observation.TestContext, nil)

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	userID := ct.CreateTestUser(t, db, true).ID
	namespaceID := relay.MarshalID("User", userID)

	input := map[string]any{
		"namespace": namespaceID,
		"name":      "my-batch-change",
	}

	var response struct{ CreateEmptyBatchChange apitest.BatchChange }
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	// First time should work because no batch change exists
	apitest.MustExec(actorCtx, t, s, input, &response, mutationCreateEmptyBatchChange)

	if response.CreateEmptyBatchChange.ID == "" {
		t.Fatalf("expected batch change to be created, but was not")
	}

	// Second time should fail because namespace + name are not unique
	errors := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateEmptyBatchChange)

	if len(errors) != 1 {
		t.Fatalf("expected single errors, but got none")
	}
	if have, want := errors[0].Message, service.ErrNameNotUnique.Error(); have != want {
		t.Fatalf("wrong error. want=%q, have=%q", want, have)
	}

	// But third time should work because a different namespace + the same name is okay
	orgID := ct.InsertTestOrg(t, db, "my-org")
	namespaceID2 := relay.MarshalID("Org", orgID)

	input2 := map[string]any{
		"namespace": namespaceID2,
		"name":      "my-batch-change",
	}

	apitest.MustExec(actorCtx, t, s, input2, &response, mutationCreateEmptyBatchChange)

	if response.CreateEmptyBatchChange.ID == "" {
		t.Fatalf("expected batch change to be created, but was not")
	}

	// This case should fail because the name fails validation
	input3 := map[string]any{
		"namespace": namespaceID,
		"name":      "not: valid:\nname",
	}

	errors = apitest.Exec(actorCtx, t, s, input3, &response, mutationCreateEmptyBatchChange)

	if len(errors) != 1 {
		t.Fatalf("expected single errors, but got none")
	}

	expError := "The batch change name can only contain word characters, dots and dashes."
	if have, want := errors[0].Message, expError; !strings.Contains(have, "The batch change name can only contain word characters, dots and dashes.") {
		t.Fatalf("wrong error. want to contain=%q, have=%q", want, have)
	}
}

const mutationCreateEmptyBatchChange = `
mutation($namespace: ID!, $name: String!){
	createEmptyBatchChange(namespace: $namespace, name: $name) {
		...batchChange
	}
}
` + fragmentBatchChange

func TestUpsertEmptyBatchChange(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	cstore := store.New(db, &observation.TestContext, nil)

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)
	if err != nil {
		t.Fatal(err)
	}

	userID := ct.CreateTestUser(t, db, true).ID
	namespaceID := relay.MarshalID("User", userID)

	input := map[string]any{
		"namespace": namespaceID,
		"name":      "my-batch-change",
	}

	var response struct{ UpsertEmptyBatchChange apitest.BatchChange }
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	// First time should work because no batch change exists, so new one is created
	apitest.MustExec(actorCtx, t, s, input, &response, mutationUpsertEmptyBatchChange)

	if response.UpsertEmptyBatchChange.ID == "" {
		t.Fatalf("expected batch change to be created, but was not")
	}

	// Second time should return existing batch change
	apitest.MustExec(actorCtx, t, s, input, &response, mutationUpsertEmptyBatchChange)

	if response.UpsertEmptyBatchChange.ID == "" {
		t.Fatalf("expected existing batch change, but was not")
	}

	badInput := map[string]any{
		"namespace": "bad_namespace-id",
		"name":      "my-batch-change",
	}

	errors := apitest.Exec(actorCtx, t, s, badInput, &response, mutationUpsertEmptyBatchChange)

	if len(errors) != 1 {
		t.Fatalf("expected single errors")
	}

	wantError := "invalid ID \"bad_namespace-id\" for namespace"

	if have, want := errors[0].Message, wantError; have != want {
		t.Fatalf("wrong error. want=%q, have=%q", want, have)
	}
}

const mutationUpsertEmptyBatchChange = `
mutation($namespace: ID!, $name: String!){
	upsertEmptyBatchChange(namespace: $namespace, name: $name) {
		...batchChange
	}
}
` + fragmentBatchChange

func TestCreateBatchChange(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := ct.CreateTestUser(t, db, true).ID

	cstore := store.New(db, &observation.TestContext, nil)

	batchSpec := &btypes.BatchSpec{
		RawSpec: ct.TestRawBatchSpec,
		Spec: &batcheslib.BatchSpec{
			Name:        "my-batch-change",
			Description: "My description",
		},
		UserID:          userID,
		NamespaceUserID: userID,
	}
	if err := cstore.CreateBatchSpec(ctx, batchSpec); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	input := map[string]any{
		"batchSpec": string(marshalBatchSpecRandID(batchSpec.RandID)),
	}

	var response struct{ CreateBatchChange apitest.BatchChange }
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	// First time it should work, because no batch change exists
	apitest.MustExec(actorCtx, t, s, input, &response, mutationCreateBatchChange)

	if response.CreateBatchChange.ID == "" {
		t.Fatalf("expected batch change to be created, but was not")
	}

	// Second time it should fail
	errors := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateBatchChange)

	if len(errors) != 1 {
		t.Fatalf("expected single errors, but got none")
	}
	if have, want := errors[0].Message, service.ErrMatchingBatchChangeExists.Error(); have != want {
		t.Fatalf("wrong error. want=%q, have=%q", want, have)
	}
}

const mutationCreateBatchChange = `
mutation($batchSpec: ID!, $publicationStates: [ChangesetSpecPublicationStateInput!]){
	createBatchChange(batchSpec: $batchSpec, publicationStates: $publicationStates) {
		...batchChange
	}
}
` + fragmentBatchChange

func TestApplyOrCreateBatchSpecWithPublicationStates(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	// Ensure our site configuration doesn't have rollout windows so we get a
	// consistent initial state.
	ct.MockConfig(t, &conf.Unified{})

	userID := ct.CreateTestUser(t, db, true).ID
	userAPIID := string(graphqlbackend.MarshalUserID(userID))
	apiUser := &apitest.User{
		ID:         userAPIID,
		DatabaseID: userID,
		SiteAdmin:  true,
	}
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	now := timeutil.Now()
	clock := func() time.Time { return now }
	cstore := store.NewWithClock(db, &observation.TestContext, nil, clock)
	repoStore := database.ReposWith(logger, cstore)
	esStore := database.ExternalServicesWith(logger, cstore)

	repo := newGitHubTestRepo("github.com/sourcegraph/apply-create-batch-change-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	// Since apply and create are essentially the same underneath, we can test
	// them with the same test code provided we special case the response type
	// handling.
	for name, tc := range map[string]struct {
		exec func(ctx context.Context, t testing.TB, s *graphql.Schema, in map[string]any) (*apitest.BatchChange, error)
	}{
		"applyBatchChange": {
			exec: func(ctx context.Context, t testing.TB, s *graphql.Schema, in map[string]any) (*apitest.BatchChange, error) {
				var response struct{ ApplyBatchChange apitest.BatchChange }
				if errs := apitest.Exec(ctx, t, s, in, &response, mutationApplyBatchChange); errs != nil {
					return nil, errors.Newf("GraphQL errors: %v", errs)
				}
				return &response.ApplyBatchChange, nil
			},
		},
		"createBatchChange": {
			exec: func(ctx context.Context, t testing.TB, s *graphql.Schema, in map[string]any) (*apitest.BatchChange, error) {
				var response struct{ CreateBatchChange apitest.BatchChange }
				if errs := apitest.Exec(ctx, t, s, in, &response, mutationCreateBatchChange); errs != nil {
					return nil, errors.Newf("GraphQL errors: %v", errs)
				}
				return &response.CreateBatchChange, nil
			},
		},
	} {
		// Create initial specs. Note that we have to append the test case name
		// to the batch spec ID to avoid cross-contamination between the test
		// cases.
		batchSpec := ct.CreateBatchSpec(t, ctx, cstore, "batch-spec-"+name, userID)
		changesetSpec := ct.CreateChangesetSpec(t, ctx, cstore, ct.TestSpecOpts{
			User:      userID,
			Repo:      repo.ID,
			BatchSpec: batchSpec.ID,
			HeadRef:   "refs/heads/my-branch-1",
		})

		// We need a couple more changeset specs to make this useful: we need to
		// be able to test that changeset specs attached to other batch specs
		// cannot be modified, and that changeset specs with explicit published
		// fields cause errors.
		otherBatchSpec := ct.CreateBatchSpec(t, ctx, cstore, "other-batch-spec-"+name, userID)
		otherChangesetSpec := ct.CreateChangesetSpec(t, ctx, cstore, ct.TestSpecOpts{
			User:      userID,
			Repo:      repo.ID,
			BatchSpec: otherBatchSpec.ID,
			HeadRef:   "refs/heads/my-branch-2",
		})

		publishedChangesetSpec := ct.CreateChangesetSpec(t, ctx, cstore, ct.TestSpecOpts{
			User:      userID,
			Repo:      repo.ID,
			BatchSpec: batchSpec.ID,
			HeadRef:   "refs/heads/my-branch-3",
			Published: true,
		})

		t.Run(name, func(t *testing.T) {
			// Handle the interesting error cases for different
			// publicationStates inputs.
			for name, states := range map[string][]map[string]any{
				"other batch spec": {
					{
						"changesetSpec":    marshalChangesetSpecRandID(otherChangesetSpec.RandID),
						"publicationState": true,
					},
				},
				"duplicate batch specs": {
					{
						"changesetSpec":    marshalChangesetSpecRandID(changesetSpec.RandID),
						"publicationState": true,
					},
					{
						"changesetSpec":    marshalChangesetSpecRandID(changesetSpec.RandID),
						"publicationState": true,
					},
				},
				"invalid publication state": {
					{
						"changesetSpec":    marshalChangesetSpecRandID(changesetSpec.RandID),
						"publicationState": "foo",
					},
				},
				"invalid changeset spec ID": {
					{
						"changesetSpec":    "foo",
						"publicationState": true,
					},
				},
				"changeset spec with a published state": {
					{
						"changesetSpec":    marshalChangesetSpecRandID(publishedChangesetSpec.RandID),
						"publicationState": true,
					},
				},
			} {
				t.Run(name, func(t *testing.T) {
					input := map[string]any{
						"batchSpec":         string(marshalBatchSpecRandID(batchSpec.RandID)),
						"publicationStates": states,
					}
					if _, errs := tc.exec(actorCtx, t, s, input); errs == nil {
						t.Fatalf("expected errors, got none")
					}
				})
			}

			// Finally, let's actually make a legit apply.
			t.Run("success", func(t *testing.T) {
				input := map[string]any{
					"batchSpec": string(marshalBatchSpecRandID(batchSpec.RandID)),
					"publicationStates": []map[string]any{
						{
							"changesetSpec":    marshalChangesetSpecRandID(changesetSpec.RandID),
							"publicationState": true,
						},
					},
				}
				have, err := tc.exec(actorCtx, t, s, input)
				if err != nil {
					t.Error(err)
				}
				want := &apitest.BatchChange{
					ID:          have.ID,
					Name:        batchSpec.Spec.Name,
					Description: batchSpec.Spec.Description,
					Namespace: apitest.UserOrg{
						ID:         userAPIID,
						DatabaseID: userID,
						SiteAdmin:  true,
					},
					Creator:       apiUser,
					LastApplier:   apiUser,
					LastAppliedAt: marshalDateTime(t, now),
					Changesets: apitest.ChangesetConnection{
						Nodes: []apitest.Changeset{
							{Typename: "ExternalChangeset", State: string(btypes.ChangesetStateProcessing)},
							{Typename: "ExternalChangeset", State: string(btypes.ChangesetStateProcessing)},
						},
						TotalCount: 2,
					},
				}
				if diff := cmp.Diff(want, have); diff != "" {
					t.Errorf("unexpected response (-want +have):\n%s", diff)
				}
			})
		})
	}
}

func TestMoveBatchChange(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	user := ct.CreateTestUser(t, db, true)
	userID := user.ID

	orgName := "move-batch-change-test"
	orgID := ct.InsertTestOrg(t, db, orgName)

	cstore := store.New(db, &observation.TestContext, nil)

	batchSpec := &btypes.BatchSpec{
		RawSpec:         ct.TestRawBatchSpec,
		UserID:          userID,
		NamespaceUserID: userID,
	}
	if err := cstore.CreateBatchSpec(ctx, batchSpec); err != nil {
		t.Fatal(err)
	}

	batchChange := &btypes.BatchChange{
		BatchSpecID:     batchSpec.ID,
		Name:            "old-name",
		CreatorID:       userID,
		LastApplierID:   userID,
		LastAppliedAt:   time.Now(),
		NamespaceUserID: batchSpec.UserID,
	}
	if err := cstore.CreateBatchChange(ctx, batchChange); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	// Move to a new name
	batchChangeAPIID := string(marshalBatchChangeID(batchChange.ID))
	newBatchChagneName := "new-name"
	input := map[string]any{
		"batchChange": batchChangeAPIID,
		"newName":     newBatchChagneName,
	}

	var response struct{ MoveBatchChange apitest.BatchChange }
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))
	apitest.MustExec(actorCtx, t, s, input, &response, mutationMoveBatchChange)

	haveBatchChange := response.MoveBatchChange
	if diff := cmp.Diff(input["newName"], haveBatchChange.Name); diff != "" {
		t.Fatalf("unexpected name (-want +got):\n%s", diff)
	}

	wantURL := fmt.Sprintf("/users/%s/batch-changes/%s", user.Username, newBatchChagneName)
	if diff := cmp.Diff(wantURL, haveBatchChange.URL); diff != "" {
		t.Fatalf("unexpected URL (-want +got):\n%s", diff)
	}

	// Move to a new namespace
	orgAPIID := graphqlbackend.MarshalOrgID(orgID)
	input = map[string]any{
		"batchChange":  string(marshalBatchChangeID(batchChange.ID)),
		"newNamespace": orgAPIID,
	}

	apitest.MustExec(actorCtx, t, s, input, &response, mutationMoveBatchChange)

	haveBatchChange = response.MoveBatchChange
	if diff := cmp.Diff(string(orgAPIID), haveBatchChange.Namespace.ID); diff != "" {
		t.Fatalf("unexpected namespace (-want +got):\n%s", diff)
	}
	wantURL = fmt.Sprintf("/organizations/%s/batch-changes/%s", orgName, newBatchChagneName)
	if diff := cmp.Diff(wantURL, haveBatchChange.URL); diff != "" {
		t.Fatalf("unexpected URL (-want +got):\n%s", diff)
	}
}

const mutationMoveBatchChange = `
fragment u on User { id, databaseID, siteAdmin }
fragment o on Org  { id, name }

mutation($batchChange: ID!, $newName: String, $newNamespace: ID){
  moveBatchChange(batchChange: $batchChange, newName: $newName, newNamespace: $newNamespace) {
	id, name, description
	creator { ...u }
	namespace {
		... on User { ...u }
		... on Org  { ...o }
	}
	url
  }
}
`

func TestListChangesetOptsFromArgs(t *testing.T) {
	var wantFirst int32 = 10
	wantPublicationStates := []btypes.ChangesetPublicationState{
		"PUBLISHED",
		"INVALID",
	}
	haveStates := []string{"OPEN", "INVALID"}
	haveReviewStates := []string{"APPROVED", "INVALID"}
	haveCheckStates := []string{"PENDING", "INVALID"}
	wantExternalStates := []btypes.ChangesetExternalState{"OPEN"}
	wantReviewStates := []btypes.ChangesetReviewState{"APPROVED", "INVALID"}
	wantCheckStates := []btypes.ChangesetCheckState{"PENDING", "INVALID"}
	truePtr := func() *bool { val := true; return &val }()
	wantSearches := []search.TextSearchTerm{{Term: "foo"}, {Term: "bar", Not: true}}
	var batchChangeID int64 = 1
	var repoID api.RepoID = 123
	repoGraphQLID := graphqlbackend.MarshalRepositoryID(repoID)
	onlyClosable := true
	openChangsetState := "OPEN"

	tcs := []struct {
		args       *graphqlbackend.ListChangesetsArgs
		wantSafe   bool
		wantErr    string
		wantParsed store.ListChangesetsOpts
	}{
		// No args given.
		{
			args:       nil,
			wantSafe:   true,
			wantParsed: store.ListChangesetsOpts{},
		},
		// First argument is set in opts, and considered safe.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				First: wantFirst,
			},
			wantSafe:   true,
			wantParsed: store.ListChangesetsOpts{LimitOpts: store.LimitOpts{Limit: 10}},
		},
		// Setting state is safe and transferred to opts.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				State: &haveStates[0],
			},
			wantSafe: true,
			wantParsed: store.ListChangesetsOpts{
				ExternalStates:   wantExternalStates[0:1],
				PublicationState: &wantPublicationStates[0],
				ReconcilerStates: []btypes.ReconcilerState{btypes.ReconcilerStateCompleted},
			},
		},
		// Setting invalid state fails.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				State: &haveStates[1],
			},
			wantErr: "changeset state not valid",
		},
		// Setting review state is not safe and transferred to opts.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				ReviewState: &haveReviewStates[0],
			},
			wantSafe:   false,
			wantParsed: store.ListChangesetsOpts{ExternalReviewState: &wantReviewStates[0]},
		},
		// Setting invalid review state fails.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				ReviewState: &haveReviewStates[1],
			},
			wantErr: "changeset review state not valid",
		},
		// Setting check state is not safe and transferred to opts.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				CheckState: &haveCheckStates[0],
			},
			wantSafe:   false,
			wantParsed: store.ListChangesetsOpts{ExternalCheckState: &wantCheckStates[0]},
		},
		// Setting invalid check state fails.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				CheckState: &haveCheckStates[1],
			},
			wantErr: "changeset check state not valid",
		},
		// Setting OnlyPublishedByThisBatchChange true.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				OnlyPublishedByThisBatchChange: truePtr,
			},
			wantSafe: true,
			wantParsed: store.ListChangesetsOpts{
				PublicationState:     &wantPublicationStates[0],
				OwnedByBatchChangeID: batchChangeID,
			},
		},
		// Setting a positive search.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				Search: stringPtr("foo"),
			},
			wantSafe: false,
			wantParsed: store.ListChangesetsOpts{
				TextSearch: wantSearches[0:1],
			},
		},
		// Setting a negative search.
		{
			args: &graphqlbackend.ListChangesetsArgs{
				Search: stringPtr("-bar"),
			},
			wantSafe: false,
			wantParsed: store.ListChangesetsOpts{
				TextSearch: wantSearches[1:],
			},
		},
		// Setting OnlyArchived
		{
			args: &graphqlbackend.ListChangesetsArgs{
				OnlyArchived: true,
			},
			wantSafe: true,
			wantParsed: store.ListChangesetsOpts{
				OnlyArchived: true,
			},
		},
		// Setting Repo
		{
			args: &graphqlbackend.ListChangesetsArgs{
				Repo: &repoGraphQLID,
			},
			wantSafe: true,
			wantParsed: store.ListChangesetsOpts{
				RepoIDs: []api.RepoID{repoID},
			},
		},
		// onlyClosable changesets
		{
			args: &graphqlbackend.ListChangesetsArgs{
				OnlyClosable: &onlyClosable,
			},
			wantSafe: true,
			wantParsed: store.ListChangesetsOpts{
				PublicationState: &wantPublicationStates[0],
				ExternalStates: []btypes.ChangesetExternalState{
					btypes.ChangesetExternalStateDraft,
					btypes.ChangesetExternalStateOpen,
				},
				ReconcilerStates: []btypes.ReconcilerState{btypes.ReconcilerStateCompleted},
			},
		},
		// error when state and onlyClosable are not null
		{
			args: &graphqlbackend.ListChangesetsArgs{
				OnlyClosable: &onlyClosable,
				State:        &openChangsetState,
			},
			wantSafe:   false,
			wantParsed: store.ListChangesetsOpts{},
			wantErr:    "invalid combination of state and onlyClosable",
		},
	}
	for i, tc := range tcs {
		t.Run(strconv.Itoa(i), func(t *testing.T) {
			haveParsed, haveSafe, err := listChangesetOptsFromArgs(tc.args, batchChangeID)
			if tc.wantErr == "" && err != nil {
				t.Fatal(err)
			}
			haveErr := fmt.Sprintf("%v", err)
			wantErr := tc.wantErr
			if wantErr == "" {
				wantErr = "<nil>"
			}
			if have, want := haveErr, wantErr; have != want {
				t.Errorf("wrong error returned. have=%q want=%q", have, want)
			}
			if diff := cmp.Diff(haveParsed, tc.wantParsed); diff != "" {
				t.Errorf("wrong args returned. diff=%s", diff)
			}
			if have, want := haveSafe, tc.wantSafe; have != want {
				t.Errorf("wrong safe value returned. have=%t want=%t", have, want)
			}
		})
	}
}

func TestCreateBatchChangesCredential(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ct.MockRSAKeygen(t)

	logger := logtest.Scoped(t)

	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	pruneUserCredentials(t, db, nil)

	userID := ct.CreateTestUser(t, db, true).ID

	cstore := store.New(db, &observation.TestContext, nil)

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	var validationErr error
	service.Mocks.ValidateAuthenticator = func(ctx context.Context, externalServiceID, externalServiceType string, a auth.Authenticator) error {
		return validationErr
	}
	t.Cleanup(func() {
		service.Mocks.Reset()
	})

	t.Run("User credential", func(t *testing.T) {
		input := map[string]any{
			"user":                graphqlbackend.MarshalUserID(userID),
			"externalServiceKind": string(extsvc.KindGitHub),
			"externalServiceURL":  "https://github.com/",
			"credential":          "SOSECRET",
		}

		var response struct {
			CreateBatchChangesCredential apitest.BatchChangesCredential
		}
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		t.Run("validation fails", func(t *testing.T) {
			// Throw correct error when credential failed validation
			validationErr = errors.New("fake validation failed")
			t.Cleanup(func() {
				validationErr = nil
			})
			errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateCredential)

			if len(errs) != 1 {
				t.Fatalf("expected single errors, but got none")
			}
			if have, want := errs[0].Extensions["code"], "ErrVerifyCredentialFailed"; have != want {
				t.Fatalf("wrong error code. want=%q, have=%q", want, have)
			}
		})

		// First time it should work, because no credential exists
		apitest.MustExec(actorCtx, t, s, input, &response, mutationCreateCredential)

		if response.CreateBatchChangesCredential.ID == "" {
			t.Fatalf("expected credential to be created, but was not")
		}

		// Second time it should fail
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateCredential)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Extensions["code"], "ErrDuplicateCredential"; have != want {
			t.Fatalf("wrong error code. want=%q, have=%q", want, have)
		}
	})
	t.Run("Site credential", func(t *testing.T) {
		input := map[string]any{
			"user":                nil,
			"externalServiceKind": string(extsvc.KindGitHub),
			"externalServiceURL":  "https://github.com/",
			"credential":          "SOSECRET",
		}

		var response struct {
			CreateBatchChangesCredential apitest.BatchChangesCredential
		}
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		t.Run("validation fails", func(t *testing.T) {
			// Throw correct error when credential failed validation
			validationErr = errors.New("fake validation failed")
			t.Cleanup(func() {
				validationErr = nil
			})
			errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateCredential)

			if len(errs) != 1 {
				t.Fatalf("expected single errors, but got none")
			}
			if have, want := errs[0].Extensions["code"], "ErrVerifyCredentialFailed"; have != want {
				t.Fatalf("wrong error code. want=%q, have=%q", want, have)
			}
		})

		// First time it should work, because no site credential exists
		apitest.MustExec(actorCtx, t, s, input, &response, mutationCreateCredential)

		if response.CreateBatchChangesCredential.ID == "" {
			t.Fatalf("expected credential to be created, but was not")
		}

		// Second time it should fail
		errors := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateCredential)

		if len(errors) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errors[0].Extensions["code"], "ErrDuplicateCredential"; have != want {
			t.Fatalf("wrong error code. want=%q, have=%q", want, have)
		}
	})
}

const mutationCreateCredential = `
mutation($user: ID, $externalServiceKind: ExternalServiceKind!, $externalServiceURL: String!, $credential: String!) {
  createBatchChangesCredential(user: $user, externalServiceKind: $externalServiceKind, externalServiceURL: $externalServiceURL, credential: $credential) { id }
}
`

func TestDeleteBatchChangesCredential(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ct.MockRSAKeygen(t)
	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	pruneUserCredentials(t, db, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	ctx = actor.WithActor(ctx, actor.FromUser(userID))

	cstore := store.New(db, &observation.TestContext, nil)

	authenticator := &auth.OAuthBearerToken{Token: "SOSECRET"}
	userCred, err := cstore.UserCredentials().Create(ctx, database.UserCredentialScope{
		Domain:              database.UserCredentialDomainBatches,
		ExternalServiceType: extsvc.TypeGitHub,
		ExternalServiceID:   "https://github.com/",
		UserID:              userID,
	}, authenticator)
	if err != nil {
		t.Fatal(err)
	}
	siteCred := &btypes.SiteCredential{
		ExternalServiceType: extsvc.TypeGitHub,
		ExternalServiceID:   "https://github.com/",
	}
	if err := cstore.CreateSiteCredential(ctx, siteCred, authenticator); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	t.Run("User credential", func(t *testing.T) {
		input := map[string]any{
			"batchChangesCredential": marshalBatchChangesCredentialID(userCred.ID, false),
		}

		var response struct{ DeleteBatchChangesCredential apitest.EmptyResponse }
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		// First time it should work, because a credential exists
		apitest.MustExec(actorCtx, t, s, input, &response, mutationDeleteCredential)

		// Second time it should fail
		errors := apitest.Exec(actorCtx, t, s, input, &response, mutationDeleteCredential)

		if len(errors) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errors[0].Message, fmt.Sprintf("user credential not found: [%d]", userCred.ID); have != want {
			t.Fatalf("wrong error code. want=%q, have=%q", want, have)
		}
	})

	t.Run("Site credential", func(t *testing.T) {
		input := map[string]any{
			"batchChangesCredential": marshalBatchChangesCredentialID(userCred.ID, true),
		}

		var response struct{ DeleteBatchChangesCredential apitest.EmptyResponse }
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		// First time it should work, because a credential exists
		apitest.MustExec(actorCtx, t, s, input, &response, mutationDeleteCredential)

		// Second time it should fail
		errors := apitest.Exec(actorCtx, t, s, input, &response, mutationDeleteCredential)

		if len(errors) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errors[0].Message, "no results"; have != want {
			t.Fatalf("wrong error code. want=%q, have=%q", want, have)
		}
	})
}

const mutationDeleteCredential = `
mutation($batchChangesCredential: ID!) {
  deleteBatchChangesCredential(batchChangesCredential: $batchChangesCredential) { alwaysNil }
}
`

func TestCreateChangesetComments(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	cstore := store.New(db, &observation.TestContext, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	batchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-comments", userID)
	otherBatchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-comments-other", userID)
	batchChange := ct.CreateBatchChange(t, ctx, cstore, "test-comments", userID, batchSpec.ID)
	otherBatchChange := ct.CreateBatchChange(t, ctx, cstore, "test-comments-other", userID, otherBatchSpec.ID)
	repo, _ := ct.CreateTestRepo(t, ctx, db)
	changeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      batchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
	})
	otherChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
	})

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	generateInput := func() map[string]any {
		return map[string]any{
			"batchChange": marshalBatchChangeID(batchChange.ID),
			"changesets":  []string{string(marshalChangesetID(changeset.ID))},
			"body":        "test-body",
		}
	}

	var response struct {
		CreateChangesetComments apitest.BulkOperation
	}
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	t.Run("empty body fails", func(t *testing.T) {
		input := generateInput()
		input["body"] = ""
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateChangesetComments)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "empty comment body is not allowed"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("0 changesets fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateChangesetComments)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "specify at least one changeset"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("changeset in different batch change fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(otherChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCreateChangesetComments)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("runs successfully", func(t *testing.T) {
		input := generateInput()
		apitest.MustExec(actorCtx, t, s, input, &response, mutationCreateChangesetComments)

		if response.CreateChangesetComments.ID == "" {
			t.Fatalf("expected bulk operation to be created, but was not")
		}
	})
}

const mutationCreateChangesetComments = `
mutation($batchChange: ID!, $changesets: [ID!]!, $body: String!) {
    createChangesetComments(batchChange: $batchChange, changesets: $changesets, body: $body) { id }
}
`

func TestReenqueueChangesets(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	cstore := store.New(db, &observation.TestContext, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	batchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-reenqueue", userID)
	otherBatchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-reenqueue-other", userID)
	batchChange := ct.CreateBatchChange(t, ctx, cstore, "test-reenqueue", userID, batchSpec.ID)
	otherBatchChange := ct.CreateBatchChange(t, ctx, cstore, "test-reenqueue-other", userID, otherBatchSpec.ID)
	repo, _ := ct.CreateTestRepo(t, ctx, db)
	changeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      batchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateFailed,
	})
	otherChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateFailed,
	})
	successfulChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
	})

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	generateInput := func() map[string]any {
		return map[string]any{
			"batchChange": marshalBatchChangeID(batchChange.ID),
			"changesets":  []string{string(marshalChangesetID(changeset.ID))},
		}
	}

	var response struct {
		ReenqueueChangesets apitest.BulkOperation
	}
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	t.Run("0 changesets fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationReenqueueChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "specify at least one changeset"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("changeset in different batch change fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(otherChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationReenqueueChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("successful changeset fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(successfulChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationReenqueueChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("runs successfully", func(t *testing.T) {
		input := generateInput()
		apitest.MustExec(actorCtx, t, s, input, &response, mutationReenqueueChangesets)

		if response.ReenqueueChangesets.ID == "" {
			t.Fatalf("expected bulk operation to be created, but was not")
		}
	})
}

const mutationReenqueueChangesets = `
mutation($batchChange: ID!, $changesets: [ID!]!) {
    reenqueueChangesets(batchChange: $batchChange, changesets: $changesets) { id }
}
`

func TestMergeChangesets(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	cstore := store.New(db, &observation.TestContext, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	batchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-merge", userID)
	otherBatchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-merge-other", userID)
	batchChange := ct.CreateBatchChange(t, ctx, cstore, "test-merge", userID, batchSpec.ID)
	otherBatchChange := ct.CreateBatchChange(t, ctx, cstore, "test-merge-other", userID, otherBatchSpec.ID)
	repo, _ := ct.CreateTestRepo(t, ctx, db)
	changeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      batchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
		ExternalState:    btypes.ChangesetExternalStateOpen,
	})
	otherChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
		ExternalState:    btypes.ChangesetExternalStateOpen,
	})
	mergedChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
		ExternalState:    btypes.ChangesetExternalStateMerged,
	})

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	generateInput := func() map[string]any {
		return map[string]any{
			"batchChange": marshalBatchChangeID(batchChange.ID),
			"changesets":  []string{string(marshalChangesetID(changeset.ID))},
		}
	}

	var response struct {
		MergeChangesets apitest.BulkOperation
	}
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	t.Run("0 changesets fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationMergeChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "specify at least one changeset"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("changeset in different batch change fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(otherChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationMergeChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("merged changeset fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(mergedChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationMergeChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("runs successfully", func(t *testing.T) {
		input := generateInput()
		apitest.MustExec(actorCtx, t, s, input, &response, mutationMergeChangesets)

		if response.MergeChangesets.ID == "" {
			t.Fatalf("expected bulk operation to be created, but was not")
		}
	})
}

const mutationMergeChangesets = `
mutation($batchChange: ID!, $changesets: [ID!]!, $squash: Boolean = false) {
    mergeChangesets(batchChange: $batchChange, changesets: $changesets, squash: $squash) { id }
}
`

func TestCloseChangesets(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	cstore := store.New(db, &observation.TestContext, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	batchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-close", userID)
	otherBatchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-close-other", userID)
	batchChange := ct.CreateBatchChange(t, ctx, cstore, "test-close", userID, batchSpec.ID)
	otherBatchChange := ct.CreateBatchChange(t, ctx, cstore, "test-close-other", userID, otherBatchSpec.ID)
	repo, _ := ct.CreateTestRepo(t, ctx, db)
	changeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      batchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
		ExternalState:    btypes.ChangesetExternalStateOpen,
	})
	otherChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
		ExternalState:    btypes.ChangesetExternalStateOpen,
	})
	mergedChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:             repo.ID,
		BatchChange:      otherBatchChange.ID,
		PublicationState: btypes.ChangesetPublicationStatePublished,
		ReconcilerState:  btypes.ReconcilerStateCompleted,
		ExternalState:    btypes.ChangesetExternalStateMerged,
	})

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	generateInput := func() map[string]any {
		return map[string]any{
			"batchChange": marshalBatchChangeID(batchChange.ID),
			"changesets":  []string{string(marshalChangesetID(changeset.ID))},
		}
	}

	var response struct {
		CloseChangesets apitest.BulkOperation
	}
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	t.Run("0 changesets fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCloseChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "specify at least one changeset"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("changeset in different batch change fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(otherChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCloseChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("merged changeset fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(mergedChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationCloseChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("runs successfully", func(t *testing.T) {
		input := generateInput()
		apitest.MustExec(actorCtx, t, s, input, &response, mutationCloseChangesets)

		if response.CloseChangesets.ID == "" {
			t.Fatalf("expected bulk operation to be created, but was not")
		}
	})
}

const mutationCloseChangesets = `
mutation($batchChange: ID!, $changesets: [ID!]!) {
    closeChangesets(batchChange: $batchChange, changesets: $changesets) { id }
}
`

func TestPublishChangesets(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	cstore := store.New(db, &observation.TestContext, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	batchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-close", userID)
	otherBatchSpec := ct.CreateBatchSpec(t, ctx, cstore, "test-close-other", userID)
	batchChange := ct.CreateBatchChange(t, ctx, cstore, "test-close", userID, batchSpec.ID)
	otherBatchChange := ct.CreateBatchChange(t, ctx, cstore, "test-close-other", userID, otherBatchSpec.ID)
	repo, _ := ct.CreateTestRepo(t, ctx, db)
	publishableChangesetSpec := ct.CreateChangesetSpec(t, ctx, cstore, ct.TestSpecOpts{
		User:      userID,
		Repo:      repo.ID,
		BatchSpec: batchSpec.ID,
		HeadRef:   "main",
	})
	unpublishableChangesetSpec := ct.CreateChangesetSpec(t, ctx, cstore, ct.TestSpecOpts{
		User:      userID,
		Repo:      repo.ID,
		BatchSpec: batchSpec.ID,
		HeadRef:   "main",
		Published: true,
	})
	otherChangesetSpec := ct.CreateChangesetSpec(t, ctx, cstore, ct.TestSpecOpts{
		User:      userID,
		Repo:      repo.ID,
		BatchSpec: otherBatchSpec.ID,
		HeadRef:   "main",
	})
	publishableChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:            repo.ID,
		BatchChange:     batchChange.ID,
		ReconcilerState: btypes.ReconcilerStateCompleted,
		CurrentSpec:     publishableChangesetSpec.ID,
	})
	unpublishableChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:            repo.ID,
		BatchChange:     batchChange.ID,
		ReconcilerState: btypes.ReconcilerStateCompleted,
		CurrentSpec:     unpublishableChangesetSpec.ID,
	})
	otherChangeset := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
		Repo:            repo.ID,
		BatchChange:     otherBatchChange.ID,
		ReconcilerState: btypes.ReconcilerStateCompleted,
		CurrentSpec:     otherChangesetSpec.ID,
	})

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	generateInput := func() map[string]any {
		return map[string]any{
			"batchChange": marshalBatchChangeID(batchChange.ID),
			"changesets": []string{
				string(marshalChangesetID(publishableChangeset.ID)),
				string(marshalChangesetID(unpublishableChangeset.ID)),
			},
			"draft": true,
		}
	}

	var response struct {
		PublishChangesets apitest.BulkOperation
	}
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	t.Run("0 changesets fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationPublishChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "specify at least one changeset"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("changeset in different batch change fails", func(t *testing.T) {
		input := generateInput()
		input["changesets"] = []string{string(marshalChangesetID(otherChangeset.ID))}
		errs := apitest.Exec(actorCtx, t, s, input, &response, mutationPublishChangesets)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got none")
		}
		if have, want := errs[0].Message, "some changesets could not be found"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("runs successfully", func(t *testing.T) {
		input := generateInput()
		apitest.MustExec(actorCtx, t, s, input, &response, mutationPublishChangesets)

		if response.PublishChangesets.ID == "" {
			t.Fatalf("expected bulk operation to be created, but was not")
		}
	})
}

const mutationPublishChangesets = `
mutation($batchChange: ID!, $changesets: [ID!]!, $draft: Boolean!) {
	publishChangesets(batchChange: $batchChange, changesets: $changesets, draft: $draft) { id }
}
`

func TestCheckBatchChangesCredential(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ct.MockRSAKeygen(t)

	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	pruneUserCredentials(t, db, nil)

	userID := ct.CreateTestUser(t, db, true).ID
	ctx = actor.WithActor(ctx, actor.FromUser(userID))

	cstore := store.New(db, &observation.TestContext, nil)

	authenticator := &auth.OAuthBearerToken{Token: "SOSECRET"}
	userCred, err := cstore.UserCredentials().Create(ctx, database.UserCredentialScope{
		Domain:              database.UserCredentialDomainBatches,
		ExternalServiceType: extsvc.TypeGitHub,
		ExternalServiceID:   "https://github.com/",
		UserID:              userID,
	}, authenticator)
	if err != nil {
		t.Fatal(err)
	}
	siteCred := &btypes.SiteCredential{
		ExternalServiceType: extsvc.TypeGitHub,
		ExternalServiceID:   "https://github.com/",
	}
	if err := cstore.CreateSiteCredential(ctx, siteCred, authenticator); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)

	if err != nil {
		t.Fatal(err)
	}

	mockValidateAuthenticator := func(t *testing.T, err error) {
		service.Mocks.ValidateAuthenticator = func(ctx context.Context, externalServiceID, externalServiceType string, a auth.Authenticator) error {
			return err
		}
		t.Cleanup(func() {
			service.Mocks.Reset()
		})
	}

	t.Run("valid site credential", func(t *testing.T) {
		mockValidateAuthenticator(t, nil)

		input := map[string]any{
			"batchChangesCredential": marshalBatchChangesCredentialID(userCred.ID, true),
		}

		var response struct{ CheckBatchChangesCredential apitest.EmptyResponse }
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		apitest.MustExec(actorCtx, t, s, input, &response, queryCheckCredential)
	})

	t.Run("valid user credential", func(t *testing.T) {
		mockValidateAuthenticator(t, nil)

		input := map[string]any{
			"batchChangesCredential": marshalBatchChangesCredentialID(userCred.ID, false),
		}

		var response struct{ CheckBatchChangesCredential apitest.EmptyResponse }
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		apitest.MustExec(actorCtx, t, s, input, &response, queryCheckCredential)
	})

	t.Run("invalid credential", func(t *testing.T) {
		mockValidateAuthenticator(t, errors.New("credential is not authorized"))

		input := map[string]any{
			"batchChangesCredential": marshalBatchChangesCredentialID(userCred.ID, true),
		}

		var response struct{ CheckBatchChangesCredential apitest.EmptyResponse }
		actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

		errs := apitest.Exec(actorCtx, t, s, input, &response, queryCheckCredential)

		assert.Len(t, errs, 1)
		assert.Equal(t, errs[0].Extensions["code"], "ErrVerifyCredentialFailed")
	})
}

const queryCheckCredential = `
query($batchChangesCredential: ID!) {
  checkBatchChangesCredential(batchChangesCredential: $batchChangesCredential) { alwaysNil }
}
`

func TestListBatchSpecs(t *testing.T) {
	logger := logtest.Scoped(t)
	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	user := ct.CreateTestUser(t, db, true)
	userID := user.ID

	cstore := store.New(db, &observation.TestContext, nil)

	batchSpecs := make([]*btypes.BatchSpec, 0, 10)

	for i := 0; i < cap(batchSpecs); i++ {
		batchSpec := &btypes.BatchSpec{
			RawSpec:         ct.TestRawBatchSpec,
			UserID:          userID,
			NamespaceUserID: userID,
		}

		if i%2 == 0 {
			// 5 batch specs will have `createdFromRaw` set to `true` while the remaining 5
			// will be set to `false`.
			batchSpec.CreatedFromRaw = true
		}

		if err := cstore.CreateBatchSpec(ctx, batchSpec); err != nil {
			t.Fatal(err)
		}

		batchSpecs = append(batchSpecs, batchSpec)
	}

	r := &Resolver{store: cstore}
	s, err := newSchema(db, r)
	if err != nil {
		t.Fatal(err)
	}

	t.Run("include locally executed batch specs", func(t *testing.T) {
		input := map[string]any{
			"includeLocallyExecutedSpecs": true,
		}
		var response struct{ BatchSpecs apitest.BatchSpecConnection }
		apitest.MustExec(ctx, t, s, input, &response, queryListBatchSpecs)

		// All batch specs should be returned here.
		assert.Len(t, response.BatchSpecs.Nodes, len(batchSpecs))
	})

	t.Run("exclude locally executed batch specs", func(t *testing.T) {
		input := map[string]any{
			"includeLocallyExecutedSpecs": false,
		}
		var response struct{ BatchSpecs apitest.BatchSpecConnection }
		apitest.MustExec(ctx, t, s, input, &response, queryListBatchSpecs)

		// Only 5 batch specs are returned here because we excluded non-SSBC batch specs.
		assert.Len(t, response.BatchSpecs.Nodes, 5)
	})
}

const queryListBatchSpecs = `
query($includeLocallyExecutedSpecs: Boolean!) {
	batchSpecs(includeLocallyExecutedSpecs: $includeLocallyExecutedSpecs) { nodes { id } }
}
`

func stringPtr(s string) *string { return &s }

func newSchema(db database.DB, r graphqlbackend.BatchChangesResolver) (*graphql.Schema, error) {
	return graphqlbackend.NewSchema(db, r, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil)
}
