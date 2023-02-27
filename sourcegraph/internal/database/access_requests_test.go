package database

import (
	"context"
	"strconv"
	"testing"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestAccessRequests_Create(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()
	store := db.AccessRequests()

	t.Run("valid input", func(t *testing.T) {
		accessRequest, err := store.Create(ctx, &types.AccessRequest{
			Email:          "a1@example.com",
			Name:           "a1",
			AdditionalInfo: "info1",
		})
		assert.NoError(t, err)
		assert.Equal(t, "a1", accessRequest.Name)
		assert.Equal(t, "info1", accessRequest.AdditionalInfo)
		assert.Equal(t, "a1@example.com", accessRequest.Email)
		assert.Equal(t, types.AccessRequestStatusPending, accessRequest.Status)
	})

	t.Run("existing access request email", func(t *testing.T) {
		_, err := store.Create(ctx, &types.AccessRequest{
			Email:          "a2@example.com",
			Name:           "a1",
			AdditionalInfo: "info1",
		})
		assert.NoError(t, err)

		_, err = store.Create(ctx, &types.AccessRequest{
			Email:          "a2@example.com",
			Name:           "a2",
			AdditionalInfo: "info2",
		})
		assert.Error(t, err)
		assert.Equal(t, err.Error(), "cannot create user: err_access_request_with_such_email_exists")
	})

	t.Run("existing verified user email", func(t *testing.T) {
		_, err := db.Users().Create(ctx, NewUser{
			Username:        "u",
			Email:           "u@example.com",
			EmailIsVerified: true,
		})

		if err != nil {
			t.Fatal(err)
		}

		_, err = store.Create(ctx, &types.AccessRequest{
			Email:          "u@example.com",
			Name:           "a3",
			AdditionalInfo: "info3",
		})
		assert.Error(t, err)
		assert.Equal(t, err.Error(), "cannot create user: err_user_with_such_email_exists")
	})
}

func TestAccessRequests_Update(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()

	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := db.AccessRequests()

	t.Run("non-existent access request", func(t *testing.T) {
		nonExistentAccessRequestID := int32(1234)
		updated, err := store.Update(ctx, &types.AccessRequest{ID: nonExistentAccessRequestID, Status: types.AccessRequestStatusApproved})
		assert.Error(t, err)
		assert.Nil(t, updated)
		assert.Equal(t, err, &ErrAccessRequestNotFound{ID: nonExistentAccessRequestID})
	})

	t.Run("existing access request", func(t *testing.T) {
		accessRequest, err := store.Create(ctx, &types.AccessRequest{
			Email:          "a1@example.com",
			Name:           "a1",
			AdditionalInfo: "info1",
		})
		assert.NoError(t, err)
		assert.Equal(t, accessRequest.Status, types.AccessRequestStatusPending)
		updated, err := store.Update(ctx, &types.AccessRequest{ID: accessRequest.ID, Status: types.AccessRequestStatusApproved})
		assert.NotNil(t, updated)
		assert.NoError(t, err)
		assert.Equal(t, updated.Status, types.AccessRequestStatusApproved)
	})
}

func TestAccessRequests_GetByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := db.AccessRequests()

	t.Run("non-existing access request", func(t *testing.T) {
		nonExistentAccessRequestID := int32(1234)
		accessRequest, err := store.GetByID(ctx, nonExistentAccessRequestID)
		assert.Error(t, err)
		assert.Nil(t, accessRequest)
		assert.Equal(t, err, &ErrAccessRequestNotFound{ID: nonExistentAccessRequestID})
	})
	t.Run("existing access request", func(t *testing.T) {
		createdAccessRequest, err := store.Create(ctx, &types.AccessRequest{Email: "a1@example.com", Name: "a1", AdditionalInfo: "info1"})
		assert.NoError(t, err)
		accessRequest, err := store.GetByID(ctx, createdAccessRequest.ID)
		assert.NoError(t, err)
		assert.Equal(t, accessRequest, createdAccessRequest)
	})
}

func TestAccessRequests_GetByEmail(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := db.AccessRequests()

	t.Run("non-existing access request", func(t *testing.T) {
		nonExistingAccessRequestEmail := "non-existing@example"
		accessRequest, err := store.GetByEmail(ctx, nonExistingAccessRequestEmail)
		assert.Error(t, err)
		assert.Nil(t, accessRequest)
		assert.Equal(t, err, &ErrAccessRequestNotFound{Email: nonExistingAccessRequestEmail})
	})
	t.Run("existing access request", func(t *testing.T) {
		createdAccessRequest, err := store.Create(ctx, &types.AccessRequest{Email: "a1@example.com", Name: "a1", AdditionalInfo: "info1"})
		assert.NoError(t, err)
		accessRequest, err := store.GetByEmail(ctx, createdAccessRequest.Email)
		assert.NoError(t, err)
		assert.Equal(t, accessRequest, createdAccessRequest)
	})
}

func TestAccessRequests_Count(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := db.AccessRequests()

	ar1, err := store.Create(ctx, &types.AccessRequest{Email: "a1@example.com", Name: "a1", AdditionalInfo: "info1"})
	assert.NoError(t, err)
	ar2, err := store.Create(ctx, &types.AccessRequest{Email: "a2@example.com", Name: "a2", AdditionalInfo: "info2"})
	assert.NoError(t, err)
	_, err = store.Create(ctx, &types.AccessRequest{Email: "a3@example.com", Name: "a3", AdditionalInfo: "info3"})
	assert.NoError(t, err)

	t.Run("all", func(t *testing.T) {
		count, err := store.Count(ctx, &AccessRequestsFilterArgs{})
		assert.NoError(t, err)
		assert.Equal(t, count, 3)
	})

	t.Run("by status", func(t *testing.T) {
		store.Update(ctx, &types.AccessRequest{ID: ar1.ID, Status: types.AccessRequestStatusApproved})
		store.Update(ctx, &types.AccessRequest{ID: ar2.ID, Status: types.AccessRequestStatusRejected})

		pending := types.AccessRequestStatusPending
		count, err := store.Count(ctx, &AccessRequestsFilterArgs{Status: &pending})
		assert.NoError(t, err)
		assert.Equal(t, count, 1)

		rejected := types.AccessRequestStatusRejected
		count, err = store.Count(ctx, &AccessRequestsFilterArgs{Status: &rejected})
		assert.NoError(t, err)
		assert.Equal(t, count, 1)

		approved := types.AccessRequestStatusApproved
		count, err = store.Count(ctx, &AccessRequestsFilterArgs{Status: &approved})
		assert.NoError(t, err)
		assert.Equal(t, count, 1)
	})
}

func TestAccessRequests_List(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := db.AccessRequests()

	ar1, err := store.Create(ctx, &types.AccessRequest{Email: "a1@example.com", Name: "a1", AdditionalInfo: "info1"})
	assert.NoError(t, err)
	ar2, err := store.Create(ctx, &types.AccessRequest{Email: "a2@example.com", Name: "a2", AdditionalInfo: "info2"})
	assert.NoError(t, err)
	_, err = store.Create(ctx, &types.AccessRequest{Email: "a3@example.com", Name: "a3", AdditionalInfo: "info3"})
	assert.NoError(t, err)

	t.Run("all", func(t *testing.T) {
		accessRequests, err := store.List(ctx, nil, nil)
		assert.NoError(t, err)
		assert.Equal(t, len(accessRequests), 3)

		// map to names
		names := make([]string, len(accessRequests))
		for i, ar := range accessRequests {
			names[i] = ar.Name
		}

		assert.Equal(t, []string{"a3", "a2", "a1"}, names)
	})

	t.Run("order", func(t *testing.T) {
		accessRequests, err := store.List(ctx, nil, &PaginationArgs{OrderBy: OrderBy{{Field: "name"}}, Ascending: true})
		assert.NoError(t, err)
		assert.Equal(t, len(accessRequests), 3)

		// map to names
		names := make([]string, len(accessRequests))
		for i, ar := range accessRequests {
			names[i] = ar.Name
		}

		assert.Equal(t, names, []string{"a1", "a2", "a3"})
	})

	t.Run("limit & pagination", func(t *testing.T) {
		one := 1
		accessRequests, err := store.List(ctx, nil, &PaginationArgs{First: &one})
		assert.NoError(t, err)
		assert.Equal(t, len(accessRequests), 1)

		// map to names
		names := make([]string, len(accessRequests))
		for i, ar := range accessRequests {
			names[i] = ar.Name
		}

		assert.Equal(t, names, []string{"a3"})

		after := strconv.Itoa(int(accessRequests[0].ID))
		two := int(2)
		accessRequests, err = store.List(ctx, nil, &PaginationArgs{First: &two, After: &after, OrderBy: OrderBy{{Field: string(AccessRequestListID)}}})
		assert.NoError(t, err)
		assert.Equal(t, 2, len(accessRequests))

		// map to names
		names = make([]string, len(accessRequests))
		for i, ar := range accessRequests {
			names[i] = ar.Name
		}

		assert.Equal(t, names, []string{"a2", "a1"})
	})

	t.Run("by status", func(t *testing.T) {
		store.Update(ctx, &types.AccessRequest{ID: ar1.ID, Status: types.AccessRequestStatusApproved})
		store.Update(ctx, &types.AccessRequest{ID: ar2.ID, Status: types.AccessRequestStatusRejected})

		// list all pending
		pending := types.AccessRequestStatusPending
		accessRequests, err := store.List(ctx, &AccessRequestsFilterArgs{Status: &pending}, nil)
		assert.NoError(t, err)
		assert.Equal(t, len(accessRequests), 1)

		// list all rejected
		rejected := types.AccessRequestStatusRejected
		accessRequests, err = store.List(ctx, &AccessRequestsFilterArgs{Status: &rejected}, nil)
		assert.NoError(t, err)
		assert.Equal(t, len(accessRequests), 1)

		// list all approved
		approved := types.AccessRequestStatusApproved
		accessRequests, err = store.List(ctx, &AccessRequestsFilterArgs{Status: &approved}, nil)
		assert.NoError(t, err)
		assert.Equal(t, len(accessRequests), 1)
	})
}
