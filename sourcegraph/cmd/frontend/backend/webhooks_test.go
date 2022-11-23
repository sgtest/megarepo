package backend

import (
	"context"
	"testing"

	"github.com/google/uuid"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestCreateWebhook(t *testing.T) {
	users := database.NewMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{SiteAdmin: true}, nil)

	ctx := actor.WithActor(context.Background(), &actor.Actor{UID: 1})
	webhookStore := database.NewMockWebhookStore()
	whUUID, err := uuid.NewUUID()
	assert.NoError(t, err)

	db := database.NewMockDB()
	db.WebhooksFunc.SetDefaultReturn(webhookStore)
	db.UsersFunc.SetDefaultReturn(users)

	ws := NewWebhookService(db, keyring.Default())

	ghURN, err := extsvc.NewCodeHostBaseURL("https://github.com")
	require.NoError(t, err)
	testSecret := "mysecret"
	tests := []struct {
		label        string
		name         string
		codeHostKind string
		codeHostURN  string
		secret       *string
		expected     types.Webhook
		expectedErr  error
	}{
		{
			label:        "basic",
			name:         "webhook name",
			codeHostKind: extsvc.KindGitHub,
			codeHostURN:  ghURN.String(),
			secret:       &testSecret,
			expected: types.Webhook{
				ID:              1,
				Name:            "webhook name",
				UUID:            whUUID,
				CodeHostKind:    extsvc.KindGitHub,
				CodeHostURN:     ghURN,
				Secret:          nil,
				CreatedByUserID: 0,
			},
		},
		{
			label:        "invalid code host",
			codeHostKind: "InvalidKind",
			codeHostURN:  ghURN.String(),
			expectedErr:  errors.New("webhooks are not supported for code host kind InvalidKind"),
		},
		{
			label:        "secrets are not supported for code host",
			codeHostKind: extsvc.KindBitbucketCloud,
			secret:       &testSecret,
			expectedErr:  errors.New("webhooks do not support secrets for code host kind BITBUCKETCLOUD"),
		},
	}

	for _, test := range tests {
		t.Run(test.label, func(t *testing.T) {
			webhookStore.CreateFunc.SetDefaultReturn(&test.expected, nil)
			webhookStore.CreateFunc.SetDefaultHook(func(_ context.Context, _, _, _ string, _ int32, secret *encryption.Encryptable) (*types.Webhook, error) {
				if test.secret != nil {
					assert.NotZero(t, secret)
				}
				return &test.expected, nil
			})
			wh, err := ws.CreateWebhook(ctx, test.name, test.codeHostKind, test.codeHostURN, test.secret)
			if test.expectedErr == nil {
				assert.NoError(t, err)
				assert.Equal(t, test.expected.ID, wh.ID)
				assert.Equal(t, test.expected.Name, wh.Name)
				assert.Equal(t, test.expected.CodeHostKind, wh.CodeHostKind)
				assert.Equal(t, test.expected.UUID, wh.UUID)
				assert.Equal(t, test.expected.Secret, wh.Secret)
				assert.Equal(t, test.expected.CreatedByUserID, wh.CreatedByUserID)
			} else {
				assert.Equal(t, err.Error(), test.expectedErr.Error())
			}
		})
	}
}
