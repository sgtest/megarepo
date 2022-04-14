package store

import (
	"context"
	"testing"
	"time"

	"github.com/hexops/autogold"
	"github.com/hexops/valast"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/types"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
)

func TestGetDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)

	_, err := insightsDB.Exec(`
		INSERT INTO dashboard (id, title)
		VALUES (1, 'test dashboard'), (2, 'private dashboard for user 3');`)
	if err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()

	// assign some global grants just so the test can immediately fetch the created dashboard
	_, err = insightsDB.Exec(`INSERT INTO dashboard_grants (dashboard_id, global)
									VALUES (1, true)`)
	if err != nil {
		t.Fatal(err)
	}
	// assign a private grant
	_, err = insightsDB.Exec(`INSERT INTO dashboard_grants (dashboard_id, user_id)
									VALUES (2, 3)`)
	if err != nil {
		t.Fatal(err)
	}

	// assign some global grants just so the test can immediately fetch the created dashboard
	_, err = insightsDB.Exec(`INSERT INTO insight_view (id, title, description, unique_id)
									VALUES (1, 'my view', 'my description', 'unique1234')`)
	if err != nil {
		t.Fatal(err)
	}

	// assign some global grants just so the test can immediately fetch the created dashboard
	_, err = insightsDB.Exec(`INSERT INTO dashboard_insight_view (dashboard_id, insight_view_id)
									VALUES (1, 1)`)
	if err != nil {
		t.Fatal(err)
	}

	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	t.Run("test get all", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}

		autogold.Equal(t, got, autogold.ExportedOnly())
	})

	t.Run("test user 3 can see both dashboards", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{UserID: []int{3}})
		if err != nil {
			t.Fatal(err)
		}

		autogold.Equal(t, got, autogold.ExportedOnly())
	})
	t.Run("test user 3 can see both dashboards limit 1", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{UserID: []int{3}, Limit: 1})
		if err != nil {
			t.Fatal(err)
		}

		autogold.Equal(t, got, autogold.ExportedOnly())
	})
	t.Run("test user 3 can see both dashboards after 1", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{UserID: []int{3}, After: 1})
		if err != nil {
			t.Fatal(err)
		}

		autogold.Equal(t, got, autogold.ExportedOnly())
	})
}

func TestCreateDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)
	ctx := context.Background()
	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	t.Run("test create dashboard", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("BeforeCreate", []*types.Dashboard{}).Equal(t, got)

		global := true
		orgId := 1
		grants := []DashboardGrant{{nil, nil, &global}, {nil, &orgId, nil}}
		_, err = store.CreateDashboard(ctx, CreateDashboardArgs{Dashboard: types.Dashboard{ID: 1, Title: "test dashboard 1"}, Grants: grants, UserID: []int{1}, OrgID: []int{1}})
		if err != nil {
			t.Fatal(err)
		}
		got, err = store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("AfterCreateDashboard", []*types.Dashboard{{
			ID:           1,
			Title:        "test dashboard 1",
			UserIdGrants: []int64{},
			OrgIdGrants:  []int64{1},
			GlobalGrant:  true,
		}}).Equal(t, got)

		gotGrants, err := store.GetDashboardGrants(ctx, 1)
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("AfterCreateGrant", []*DashboardGrant{
			{
				Global: valast.Addr(true).(*bool),
			},
			{OrgID: valast.Addr(1).(*int)},
		}).Equal(t, gotGrants)
	})
}

func TestUpdateDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)
	ctx := context.Background()
	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	_, err := insightsDB.Exec(`
	INSERT INTO dashboard (id, title)
	VALUES (1, 'test dashboard 1'), (2, 'test dashboard 2');
	INSERT INTO dashboard_grants (dashboard_id, global)
	VALUES (1, true), (2, true);`)
	if err != nil {
		t.Fatal(err)
	}

	t.Run("test update dashboard", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("BeforeUpdate", []*types.Dashboard{
			{
				ID:           1,
				Title:        "test dashboard 1",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
			{
				ID:           2,
				Title:        "test dashboard 2",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, got)

		newTitle := "new title!"
		global := true
		userId := 1
		grants := []DashboardGrant{{nil, nil, &global}, {&userId, nil, nil}}
		_, err = store.UpdateDashboard(ctx, UpdateDashboardArgs{1, &newTitle, grants, []int{1}, []int{}})
		if err != nil {
			t.Fatal(err)
		}
		got, err = store.GetDashboards(ctx, DashboardQueryArgs{UserID: []int{1}})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("AfterUpdate", []*types.Dashboard{
			{
				ID:           1,
				Title:        "new title!",
				UserIdGrants: []int64{1},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
			{
				ID:           2,
				Title:        "test dashboard 2",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, got)
	})
}

func TestDeleteDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)
	ctx := context.Background()

	_, err := insightsDB.Exec(`
		INSERT INTO dashboard (id, title)
		VALUES (1, 'test dashboard 1'), (2, 'test dashboard 2');
		INSERT INTO dashboard_grants (dashboard_id, global)
		VALUES (1, true), (2, true);`)
	if err != nil {
		t.Fatal(err)
	}

	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	t.Run("test delete dashboard", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("BeforeDelete", []*types.Dashboard{
			{
				ID:           1,
				Title:        "test dashboard 1",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
			{
				ID:           2,
				Title:        "test dashboard 2",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, got)

		err = store.DeleteDashboard(ctx, 1)
		if err != nil {
			t.Fatal(err)
		}
		got, err = store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("AfterDelete", []*types.Dashboard{{
			ID:           2,
			Title:        "test dashboard 2",
			UserIdGrants: []int64{},
			OrgIdGrants:  []int64{},
			GlobalGrant:  true,
		}}).Equal(t, got)
	})
}

func TestRestoreDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)
	ctx := context.Background()

	_, err := insightsDB.Exec(`
		INSERT INTO dashboard (id, title, deleted_at)
		VALUES (1, 'test dashboard 1', NULL), (2, 'test dashboard 2', NOW());
		INSERT INTO dashboard_grants (dashboard_id, global)
		VALUES (1, true), (2, true);`)
	if err != nil {
		t.Fatal(err)
	}

	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	t.Run("test restore dashboard", func(t *testing.T) {
		got, err := store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("BeforeRestore", []*types.Dashboard{
			{
				ID:           1,
				Title:        "test dashboard 1",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, got)

		err = store.RestoreDashboard(ctx, 2)
		if err != nil {
			t.Fatal(err)
		}
		got, err = store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("AfterRestore", []*types.Dashboard{
			{
				ID:           1,
				Title:        "test dashboard 1",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
			{
				ID:           2,
				Title:        "test dashboard 2",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, got)
	})
}

func TestAddViewsToDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)
	ctx := context.Background()

	_, err := insightsDB.Exec(`
		INSERT INTO dashboard (id, title)
		VALUES (1, 'test dashboard 1'), (2, 'test dashboard 2');
		INSERT INTO dashboard_grants (dashboard_id, global)
		VALUES (1, true), (2, true);`)
	if err != nil {
		t.Fatal(err)
	}

	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	t.Run("create and add view to dashboard", func(t *testing.T) {
		insightStore := NewInsightStore(insightsDB)
		view1, err := insightStore.CreateView(ctx, types.InsightView{
			Title:            "great view",
			Description:      "my view",
			UniqueID:         "view1234567",
			PresentationType: types.Line,
		}, []InsightViewGrant{GlobalGrant()})
		if err != nil {
			t.Fatal(err)
		}
		view2, err := insightStore.CreateView(ctx, types.InsightView{
			Title:            "great view 2",
			Description:      "my view",
			UniqueID:         "view1234567-2",
			PresentationType: types.Line,
		}, []InsightViewGrant{GlobalGrant()})
		if err != nil {
			t.Fatal(err)
		}

		dashboards, err := store.GetDashboards(ctx, DashboardQueryArgs{ID: []int{1}})
		if err != nil || len(dashboards) != 1 {
			t.Errorf("failed to fetch dashboard before adding insight")
		}

		dashboard := dashboards[0]
		if len(dashboard.InsightIDs) != 0 {
			t.Errorf("unexpected value for insight views on dashboard before adding view")
		}
		err = store.AddViewsToDashboard(ctx, dashboard.ID, []string{view2.UniqueID, view1.UniqueID})
		if err != nil {
			t.Errorf("failed to add view to dashboard")
		}
		dashboards, err = store.GetDashboards(ctx, DashboardQueryArgs{ID: []int{1}})
		if err != nil || len(dashboards) != 1 {
			t.Errorf("failed to fetch dashboard after adding insight")
		}
		got := dashboards[0]
		autogold.Equal(t, got, autogold.ExportedOnly())
	})
}

func TestRemoveViewsFromDashboard(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Now().Truncate(time.Microsecond).Round(0)
	ctx := context.Background()

	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}

	insightStore := NewInsightStore(insightsDB)

	view, err := insightStore.CreateView(ctx, types.InsightView{
		Title:            "view1",
		Description:      "view1",
		UniqueID:         "view1",
		PresentationType: types.Line,
	}, []InsightViewGrant{GlobalGrant()})
	if err != nil {
		t.Fatal(err)
	}

	_, err = store.CreateDashboard(ctx, CreateDashboardArgs{
		Dashboard: types.Dashboard{Title: "first", InsightIDs: []string{view.UniqueID}},
		Grants:    []DashboardGrant{GlobalDashboardGrant()},
		UserID:    []int{1},
		OrgID:     []int{1}})
	if err != nil {
		t.Fatal(err)
	}
	second, err := store.CreateDashboard(ctx, CreateDashboardArgs{
		Dashboard: types.Dashboard{Title: "second", InsightIDs: []string{view.UniqueID}},
		Grants:    []DashboardGrant{GlobalDashboardGrant()},
		UserID:    []int{1},
		OrgID:     []int{1}})
	if err != nil {
		t.Fatal(err)
	}

	t.Run("remove view from one dashboard only", func(t *testing.T) {
		dashboards, err := store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("dashboards before removing a view", []*types.Dashboard{
			{
				ID:           1,
				Title:        "first",
				InsightIDs:   []string{"view1"},
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
			{
				ID:           2,
				Title:        "second",
				InsightIDs:   []string{"view1"},
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, dashboards)

		err = store.RemoveViewsFromDashboard(ctx, second.ID, []string{view.UniqueID})
		if err != nil {
			t.Fatal(err)
		}
		dashboards, err = store.GetDashboards(ctx, DashboardQueryArgs{})
		if err != nil {
			t.Fatal(err)
		}
		autogold.Want("dashboards after removing a view", []*types.Dashboard{
			{
				ID:           1,
				Title:        "first",
				InsightIDs:   []string{"view1"},
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
			{
				ID:           2,
				Title:        "second",
				UserIdGrants: []int64{},
				OrgIdGrants:  []int64{},
				GlobalGrant:  true,
			},
		}).Equal(t, dashboards)
	})
}

func TestHasDashboardPermission(t *testing.T) {
	insightsDB := dbtest.NewInsightsDB(t)
	now := time.Date(2021, 12, 1, 0, 0, 0, 0, time.UTC).Truncate(time.Microsecond).Round(0)
	ctx := context.Background()
	store := NewDashboardStore(insightsDB)
	store.Now = func() time.Time {
		return now
	}
	created, err := store.CreateDashboard(ctx, CreateDashboardArgs{
		Dashboard: types.Dashboard{
			Title: "test dashboard 123",
			Save:  true,
		},
		Grants: []DashboardGrant{UserDashboardGrant(1), OrgDashboardGrant(5)},
		UserID: []int{1}, // this is a weird thing I'd love to get rid of, but for now this will cause the db to return
	})
	if err != nil {
		t.Fatal(err)
	}

	if created == nil {
		t.Fatalf("nil dashboard")
	}

	second, err := store.CreateDashboard(ctx, CreateDashboardArgs{
		Dashboard: types.Dashboard{
			Title: "second test dashboard",
			Save:  true,
		},
		Grants: []DashboardGrant{UserDashboardGrant(2), OrgDashboardGrant(5)},
		UserID: []int{2}, // this is a weird thing I'd love to get rid of, but for now this will cause the db to return
	})
	if err != nil {
		t.Fatal(err)
	}

	if second == nil {
		t.Fatalf("nil dashboard")
	}

	tests := []struct {
		name                 string
		shouldHavePermission bool
		userIds              []int
		orgIds               []int
		dashboardIDs         []int
	}{
		{
			name:                 "user 1 has access to dashboard",
			shouldHavePermission: true,
			userIds:              []int{1},
			orgIds:               nil,
			dashboardIDs:         []int{created.ID},
		},
		{
			name:                 "user 3 does not have access to dashboard",
			shouldHavePermission: false,
			userIds:              []int{3},
			orgIds:               nil,
			dashboardIDs:         []int{created.ID},
		},
		{
			name:                 "org 5 has access to dashboard",
			shouldHavePermission: true,
			userIds:              nil,
			orgIds:               []int{5},
			dashboardIDs:         []int{created.ID},
		},
		{
			name:                 "org 7 does not have access to dashboard",
			shouldHavePermission: false,
			userIds:              nil,
			orgIds:               []int{7},
			dashboardIDs:         []int{created.ID},
		},
		{
			name:                 "no access when dashboard does not exist",
			shouldHavePermission: false,
			userIds:              []int{3},
			orgIds:               []int{5},
			dashboardIDs:         []int{-2},
		},
		{
			name:                 "user 1 has access to one of two dashboards",
			shouldHavePermission: false,
			userIds:              []int{1},
			orgIds:               nil,
			dashboardIDs:         []int{created.ID, second.ID},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := store.HasDashboardPermission(ctx, test.dashboardIDs, test.userIds, test.orgIds)
			if err != nil {
				t.Error(err)
			}
			want := test.shouldHavePermission
			if want != got {
				t.Errorf("unexpected dashboard access result from HasDashboardPermission: want: %v got: %v", want, got)
			}
		})
	}
}
