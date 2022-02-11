package database

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
)

func TestCodeMonitorStoreSlackWebhooks(t *testing.T) {
	ctx := context.Background()
	url1 := "https://icanhazcheezburger.com/slack_webhook"
	url2 := "https://icanthazcheezburger.com/slack_webhook"

	t.Run("CreateThenGet", func(t *testing.T) {
		t.Parallel()

		db := database.NewDB(dbtest.NewDB(t))
		_, _, _, ctx := newTestUser(ctx, t, db)
		s := CodeMonitors(db)
		fixtures, err := s.insertTestMonitor(ctx, t)
		require.NoError(t, err)

		action, err := s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url1)
		require.NoError(t, err)

		got, err := s.GetSlackWebhookAction(ctx, action.ID)
		require.NoError(t, err)

		require.Equal(t, action, got)
	})

	t.Run("CreateUpdateGet", func(t *testing.T) {
		t.Parallel()

		db := database.NewDB(dbtest.NewDB(t))
		_, _, _, ctx := newTestUser(ctx, t, db)
		s := CodeMonitors(db)
		fixtures, err := s.insertTestMonitor(ctx, t)
		require.NoError(t, err)

		action, err := s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url1)
		require.NoError(t, err)

		updated, err := s.UpdateSlackWebhookAction(ctx, action.ID, false, false, url2)
		require.NoError(t, err)
		require.Equal(t, false, updated.Enabled)
		require.Equal(t, url2, updated.URL)

		got, err := s.GetSlackWebhookAction(ctx, action.ID)
		require.NoError(t, err)
		require.Equal(t, updated, got)
	})

	t.Run("ErrorOnUpdateNonexistent", func(t *testing.T) {
		t.Parallel()

		db := database.NewDB(dbtest.NewDB(t))
		_, _, _, ctx := newTestUser(ctx, t, db)
		s := CodeMonitors(db)

		_, err := s.UpdateSlackWebhookAction(ctx, 383838, false, false, url2)
		require.Error(t, err)
	})

	t.Run("CreateDeleteGet", func(t *testing.T) {
		t.Parallel()

		db := database.NewDB(dbtest.NewDB(t))
		_, _, _, ctx := newTestUser(ctx, t, db)
		s := CodeMonitors(db)
		fixtures, err := s.insertTestMonitor(ctx, t)
		require.NoError(t, err)

		action1, err := s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url1)
		require.NoError(t, err)

		action2, err := s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url1)
		require.NoError(t, err)

		err = s.DeleteSlackWebhookActions(ctx, fixtures.monitor.ID, action1.ID)
		require.NoError(t, err)

		_, err = s.GetSlackWebhookAction(ctx, action1.ID)
		require.Error(t, err)

		_, err = s.GetSlackWebhookAction(ctx, action2.ID)
		require.NoError(t, err)
	})

	t.Run("CountCreateCount", func(t *testing.T) {
		t.Parallel()

		db := database.NewDB(dbtest.NewDB(t))
		_, _, _, ctx := newTestUser(ctx, t, db)
		s := CodeMonitors(db)
		fixtures, err := s.insertTestMonitor(ctx, t)
		require.NoError(t, err)

		count, err := s.CountSlackWebhookActions(ctx, fixtures.monitor.ID)
		require.NoError(t, err)
		require.Equal(t, 0, count)

		_, err = s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url1)
		require.NoError(t, err)

		count, err = s.CountSlackWebhookActions(ctx, fixtures.monitor.ID)
		require.NoError(t, err)
		require.Equal(t, 1, count)
	})

	t.Run("ListCreateList", func(t *testing.T) {
		t.Parallel()

		db := database.NewDB(dbtest.NewDB(t))
		_, _, _, ctx := newTestUser(ctx, t, db)
		s := CodeMonitors(db)
		fixtures, err := s.insertTestMonitor(ctx, t)
		require.NoError(t, err)

		actions, err := s.ListSlackWebhookActions(ctx, ListActionsOpts{MonitorID: &fixtures.monitor.ID})
		require.NoError(t, err)
		require.Len(t, actions, 0)

		_, err = s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url1)
		require.NoError(t, err)

		_, err = s.CreateSlackWebhookAction(ctx, fixtures.monitor.ID, true, false, url2)
		require.NoError(t, err)

		actions2, err := s.ListSlackWebhookActions(ctx, ListActionsOpts{MonitorID: &fixtures.monitor.ID})
		require.NoError(t, err)
		require.Len(t, actions2, 2)

		first := 1
		actions3, err := s.ListSlackWebhookActions(ctx, ListActionsOpts{MonitorID: &fixtures.monitor.ID, First: &first})
		require.NoError(t, err)
		require.Len(t, actions3, 1)
	})
}
