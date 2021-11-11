package productsubscription

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/internal/database/dbmock"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
)

func TestProductSubscription_Account(t *testing.T) {
	t.Run("user not found should be ignored", func(t *testing.T) {
		users := dbmock.NewMockUserStore()
		users.GetByIDFunc.SetDefaultReturn(nil, &errcode.Mock{IsNotFound: true})

		db := dbmock.NewMockDB()
		db.UsersFunc.SetDefaultReturn(users)

		_, err := (&productSubscription{v: &dbSubscription{UserID: 1}, db: db}).Account(context.Background())
		assert.Nil(t, err)
	})
}
