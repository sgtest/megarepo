package productsubscription

import (
	"context"
	"testing"

	"github.com/hexops/autogold/v2"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestServiceAccountOrOwnerOrSiteAdmin(t *testing.T) {
	var actorID, anotherID int32 = 1, 2
	for _, tc := range []struct {
		name           string
		featureFlags   map[string]bool
		actorSiteAdmin bool

		ownerUserID            *int32
		serviceAccountCanWrite bool

		wantErr autogold.Value
	}{
		{
			name: "reader service account",
			featureFlags: map[string]bool{
				featureFlagProductSubscriptionsReaderServiceAccount: true,
			},
			wantErr: nil,
		},
		{
			name: "service account",
			featureFlags: map[string]bool{
				featureFlagProductSubscriptionsServiceAccount: true,
			},
			wantErr: nil,
		},
		{
			name:        "same user",
			ownerUserID: &actorID,
			wantErr:     nil,
		},
		{
			name:        "different user",
			ownerUserID: &anotherID,
			wantErr:     autogold.Expect("must be authenticated as the authorized user or site admin"),
		},
		{
			name:           "site admin",
			actorSiteAdmin: true,
			wantErr:        nil,
		},
		{
			name:           "site admin can access another user",
			actorSiteAdmin: true,
			ownerUserID:    &anotherID,
			wantErr:        nil,
		},
		{
			name:    "not a site admin, not accessing a user-specific resource",
			wantErr: autogold.Expect("must be site admin"),
		},
		{
			name: "service account needs writer flag",
			featureFlags: map[string]bool{
				featureFlagProductSubscriptionsReaderServiceAccount: true,
			},
			serviceAccountCanWrite: true,
			wantErr:                autogold.Expect("must be site admin"),
		},
		{
			name: "service account fulfills writer flag",
			featureFlags: map[string]bool{
				featureFlagProductSubscriptionsServiceAccount: true,
			},
			serviceAccountCanWrite: true,
			wantErr:                nil,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			tc := tc
			t.Parallel()

			db := database.NewMockDB()
			mockUsers := database.NewMockUserStore()

			user := &types.User{ID: actorID, SiteAdmin: tc.actorSiteAdmin}
			mockUsers.GetByCurrentAuthUserFunc.SetDefaultReturn(user, nil)
			mockUsers.GetByIDFunc.SetDefaultReturn(user, nil)

			db.UsersFunc.SetDefaultReturn(mockUsers)

			ctx := featureflag.WithFlags(context.Background(),
				featureflag.NewMemoryStore(tc.featureFlags, nil, nil))

			err := serviceAccountOrOwnerOrSiteAdmin(
				actor.WithActor(ctx, &actor.Actor{UID: actorID}),
				db,
				tc.ownerUserID,
				tc.serviceAccountCanWrite,
			)
			if tc.wantErr != nil {
				require.Error(t, err)
				tc.wantErr.Equal(t, err.Error())
			} else {
				require.NoError(t, err)
			}
		})
	}
}
