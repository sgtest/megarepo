package graphql

import (
	"context"
	"encoding/base64"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	resolvermocks "github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/resolvers/mocks"
	store "github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func init() {
	autoIndexingEnabled = func() bool { return true }
}

func TestDeleteLSIFUpload(t *testing.T) {
	users := database.NewStrictMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{SiteAdmin: true}, nil)

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)

	id := graphql.ID(base64.StdEncoding.EncodeToString([]byte("LSIFUpload:42")))
	mockResolver := resolvermocks.NewMockResolver()

	if _, err := NewResolver(db, nil, mockResolver, &observation.TestContext).DeleteLSIFUpload(context.Background(), &struct{ ID graphql.ID }{id}); err != nil {
		t.Fatalf("unexpected error: %s", err)
	}

	if len(mockResolver.DeleteUploadByIDFunc.History()) != 1 {
		t.Fatalf("unexpected call count. want=%d have=%d", 1, len(mockResolver.DeleteUploadByIDFunc.History()))
	}
	if val := mockResolver.DeleteUploadByIDFunc.History()[0].Arg1; val != 42 {
		t.Fatalf("unexpected upload id. want=%d have=%d", 42, val)
	}
}

func TestDeleteLSIFUploadUnauthenticated(t *testing.T) {
	db := database.NewDB(nil)

	id := graphql.ID(base64.StdEncoding.EncodeToString([]byte("LSIFUpload:42")))
	mockResolver := resolvermocks.NewMockResolver()

	if _, err := NewResolver(db, nil, mockResolver, &observation.TestContext).DeleteLSIFUpload(context.Background(), &struct{ ID graphql.ID }{id}); err != backend.ErrNotAuthenticated {
		t.Errorf("unexpected error. want=%q have=%q", backend.ErrNotAuthenticated, err)
	}
}

func TestDeleteLSIFIndex(t *testing.T) {
	users := database.NewStrictMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{SiteAdmin: true}, nil)

	db := database.NewStrictMockDB()
	db.UsersFunc.SetDefaultReturn(users)

	id := graphql.ID(base64.StdEncoding.EncodeToString([]byte("LSIFIndex:42")))
	mockResolver := resolvermocks.NewMockResolver()

	if _, err := NewResolver(db, nil, mockResolver, &observation.TestContext).DeleteLSIFIndex(context.Background(), &struct{ ID graphql.ID }{id}); err != nil {
		t.Fatalf("unexpected error: %s", err)
	}

	if len(mockResolver.DeleteIndexByIDFunc.History()) != 1 {
		t.Fatalf("unexpected call count. want=%d have=%d", 1, len(mockResolver.DeleteIndexByIDFunc.History()))
	}
	if val := mockResolver.DeleteIndexByIDFunc.History()[0].Arg1; val != 42 {
		t.Fatalf("unexpected index id. want=%d have=%d", 42, val)
	}
}

func TestDeleteLSIFIndexUnauthenticated(t *testing.T) {
	db := database.NewDB(nil)

	id := graphql.ID(base64.StdEncoding.EncodeToString([]byte("LSIFIndex:42")))
	mockResolver := resolvermocks.NewMockResolver()

	if _, err := NewResolver(db, nil, mockResolver, &observation.TestContext).DeleteLSIFIndex(context.Background(), &struct{ ID graphql.ID }{id}); err != backend.ErrNotAuthenticated {
		t.Errorf("unexpected error. want=%q have=%q", backend.ErrNotAuthenticated, err)
	}
}

func TestMakeGetUploadsOptions(t *testing.T) {
	opts, err := makeGetUploadsOptions(&gql.LSIFRepositoryUploadsQueryArgs{
		LSIFUploadsQueryArgs: &gql.LSIFUploadsQueryArgs{
			ConnectionArgs: graphqlutil.ConnectionArgs{
				First: intPtr(5),
			},
			Query:           strPtr("q"),
			State:           strPtr("s"),
			IsLatestForRepo: boolPtr(true),
			After:           graphqlutil.EncodeIntCursor(intPtr(25)).EndCursor(),
		},
		RepositoryID: graphql.ID(base64.StdEncoding.EncodeToString([]byte("Repo:50"))),
	})
	if err != nil {
		t.Fatalf("unexpected error making options: %s", err)
	}

	expected := store.GetUploadsOptions{
		RepositoryID: 50,
		State:        "s",
		Term:         "q",
		VisibleAtTip: true,
		Limit:        5,
		Offset:       25,
		AllowExpired: true,
	}
	if diff := cmp.Diff(expected, opts); diff != "" {
		t.Errorf("unexpected opts (-want +got):\n%s", diff)
	}
}

func TestMakeGetUploadsOptionsDefaults(t *testing.T) {
	opts, err := makeGetUploadsOptions(&gql.LSIFRepositoryUploadsQueryArgs{
		LSIFUploadsQueryArgs: &gql.LSIFUploadsQueryArgs{},
	})
	if err != nil {
		t.Fatalf("unexpected error making options: %s", err)
	}

	expected := store.GetUploadsOptions{
		RepositoryID: 0,
		State:        "",
		Term:         "",
		VisibleAtTip: false,
		Limit:        DefaultUploadPageSize,
		Offset:       0,
		AllowExpired: true,
	}
	if diff := cmp.Diff(expected, opts); diff != "" {
		t.Errorf("unexpected opts (-want +got):\n%s", diff)
	}
}

func TestMakeGetIndexesOptions(t *testing.T) {
	opts, err := makeGetIndexesOptions(&gql.LSIFRepositoryIndexesQueryArgs{
		LSIFIndexesQueryArgs: &gql.LSIFIndexesQueryArgs{
			ConnectionArgs: graphqlutil.ConnectionArgs{
				First: intPtr(5),
			},
			Query: strPtr("q"),
			State: strPtr("s"),
			After: graphqlutil.EncodeIntCursor(intPtr(25)).EndCursor(),
		},
		RepositoryID: graphql.ID(base64.StdEncoding.EncodeToString([]byte("Repo:50"))),
	})
	if err != nil {
		t.Fatalf("unexpected error making options: %s", err)
	}

	expected := store.GetIndexesOptions{
		RepositoryID: 50,
		State:        "s",
		Term:         "q",
		Limit:        5,
		Offset:       25,
	}
	if diff := cmp.Diff(expected, opts); diff != "" {
		t.Errorf("unexpected opts (-want +got):\n%s", diff)
	}
}

func TestMakeGetIndexesOptionsDefaults(t *testing.T) {
	opts, err := makeGetIndexesOptions(&gql.LSIFRepositoryIndexesQueryArgs{
		LSIFIndexesQueryArgs: &gql.LSIFIndexesQueryArgs{},
	})
	if err != nil {
		t.Fatalf("unexpected error making options: %s", err)
	}

	expected := store.GetIndexesOptions{
		RepositoryID: 0,
		State:        "",
		Term:         "",
		Limit:        DefaultIndexPageSize,
		Offset:       0,
	}
	if diff := cmp.Diff(expected, opts); diff != "" {
		t.Errorf("unexpected opts (-want +got):\n%s", diff)
	}
}
