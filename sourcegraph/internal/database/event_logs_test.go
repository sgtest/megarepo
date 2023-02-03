package database

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math/rand"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.org/x/sync/errgroup"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSanitizeEventURL(t *testing.T) {
	cases := []struct {
		input       string
		externalURL string
		output      string
	}{{
		input:       "https://about.sourcegraph.com/test", //CI:URL_OK
		externalURL: "https://sourcegraph.com",
		output:      "https://about.sourcegraph.com/test", //CI:URL_OK
	}, {
		input:       "https://test.sourcegraph.com/test",
		externalURL: "https://sourcegraph.com",
		output:      "https://test.sourcegraph.com/test",
	}, {
		input:       "https://test.sourcegraph.com/test",
		externalURL: "https://customerinstance.com",
		output:      "https://test.sourcegraph.com/test",
	}, {
		input:       "",
		externalURL: "https://customerinstance.com",
		output:      "",
	}, {
		input:       "https://github.com/my-private-info",
		externalURL: "https://customerinstance.com",
		output:      "",
	}, {
		input:       "https://github.com/my-private-info",
		externalURL: "https://sourcegraph.com",
		output:      "",
	}, {
		input:       "invalid url",
		externalURL: "https://sourcegraph.com",
		output:      "",
	}}

	for _, tc := range cases {
		t.Run("", func(t *testing.T) {
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					ExternalURL: tc.externalURL,
				},
			})
			got := SanitizeEventURL(tc.input)
			require.Equal(t, tc.output, got)
		})
	}
}

func TestEventLogs_ValidInfo(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	var testCases = []struct {
		name  string
		event *Event
		err   string // Stringified error
	}{
		{
			name:  "EmptyName",
			event: &Event{UserID: 1, URL: "http://sourcegraph.com", Source: "WEB"},
			err:   `inserter.Flush: ERROR: new row for relation "event_logs" violates check constraint "event_logs_check_name_not_empty" (SQLSTATE 23514)`,
		},
		{
			name:  "InvalidUser",
			event: &Event{Name: "test_event", URL: "http://sourcegraph.com", Source: "WEB"},
			err:   `inserter.Flush: ERROR: new row for relation "event_logs" violates check constraint "event_logs_check_has_user" (SQLSTATE 23514)`,
		},
		{
			name:  "EmptySource",
			event: &Event{Name: "test_event", URL: "http://sourcegraph.com", UserID: 1},
			err:   `inserter.Flush: ERROR: new row for relation "event_logs" violates check constraint "event_logs_check_source_not_empty" (SQLSTATE 23514)`,
		},
		{
			name:  "ValidInsert",
			event: &Event{Name: "test_event", UserID: 1, URL: "http://sourcegraph.com", Source: "WEB"},
			err:   "<nil>",
		},
	}
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			err := db.EventLogs().Insert(ctx, tc.event)

			if have, want := fmt.Sprint(errors.Unwrap(err)), tc.err; have != want {
				t.Errorf("have %+v, want %+v", have, want)
			}
		})
	}
}

func TestEventLogs_CountUsersWithSetting(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	usersStore := db.Users()
	settingsStore := db.TemporarySettings()
	eventLogsStore := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}

	for i := 0; i < 24; i++ {
		user, err := usersStore.Create(ctx, NewUser{Username: fmt.Sprintf("u%d", i)})
		if err != nil {
			t.Fatal(err)
		}

		settings := fmt.Sprintf("{%s}", strings.Join([]string{
			fmt.Sprintf(`"foo": %d`, user.ID%7),
			fmt.Sprintf(`"bar": "%d"`, user.ID%5),
			fmt.Sprintf(`"baz": %v`, user.ID%2 == 0),
		}, ", "))

		if err := settingsStore.OverwriteTemporarySettings(ctx, user.ID, settings); err != nil {
			t.Fatal(err)
		}
	}

	for _, expectedCount := range []struct {
		key           string
		value         any
		expectedCount int
	}{
		// foo, ints
		{"foo", 0, 3},
		{"foo", 1, 4},
		{"foo", 2, 4},
		{"foo", 3, 4},
		{"foo", 4, 3},
		{"foo", 5, 3},
		{"foo", 6, 3},
		{"foo", 7, 0}, // none

		// bar, strings
		{"bar", strconv.Itoa(0), 4},
		{"bar", strconv.Itoa(1), 5},
		{"bar", strconv.Itoa(2), 5},
		{"bar", strconv.Itoa(3), 5},
		{"bar", strconv.Itoa(4), 5},
		{"bar", strconv.Itoa(5), 0}, // none

		// baz, bools
		{"baz", true, 12},
		{"baz", false, 12},
		{"baz", nil, 0}, // none
	} {
		count, err := eventLogsStore.CountUsersWithSetting(ctx, expectedCount.key, expectedCount.value)
		if err != nil {
			t.Fatal(err)
		}

		if count != expectedCount.expectedCount {
			t.Errorf("unexpected count for %q = %v. want=%d have=%d", expectedCount.key, expectedCount.value, expectedCount.expectedCount, count)
		}
	}
}

func TestEventLogs_SiteUsageMultiplePeriods(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Several of the events will belong to Sourcegraph employee admin user and Sourcegraph Operator user account
	sgAdmin, err := db.Users().Create(ctx, NewUser{Username: "sourcegraph-admin"})
	require.NoError(t, err)
	err = db.UserEmails().Add(ctx, sgAdmin.ID, "admin@sourcegraph.com", nil)
	require.NoError(t, err)
	soLoganID, err := db.UserExternalAccounts().CreateUserAndSave(
		ctx,
		NewUser{
			Username: "sourcegraph-operator-logan",
		},
		extsvc.AccountSpec{
			ServiceType: "sourcegraph-operator",
		},
		extsvc.AccountData{},
	)
	require.NoError(t, err)

	user1, err := db.Users().Create(ctx, NewUser{Username: "a"})
	require.NoError(t, err)
	user2, err := db.Users().Create(ctx, NewUser{Username: "b"})
	require.NoError(t, err)
	user3, err := db.Users().Create(ctx, NewUser{Username: "c"})
	require.NoError(t, err)
	user4, err := db.Users().Create(ctx, NewUser{Username: "d"})
	require.NoError(t, err)

	now := time.Now()
	startDate, _ := calcStartDate(now, Daily, 3)
	secondDay := startDate.Add(time.Hour * 24)
	thirdDay := startDate.Add(time.Hour * 24 * 2)

	soPublicArgument := json.RawMessage(fmt.Sprintf(`{"%s": true}`, EventLogsSourcegraphOperatorKey))
	events := []*Event{
		makeTestEvent(&Event{UserID: uint32(sgAdmin.ID), Timestamp: startDate}),
		makeTestEvent(&Event{UserID: uint32(sgAdmin.ID), Timestamp: startDate}),
		makeTestEvent(&Event{UserID: uint32(soLoganID.ID), Timestamp: startDate, PublicArgument: soPublicArgument}),
		makeTestEvent(&Event{UserID: uint32(soLoganID.ID), Timestamp: startDate, PublicArgument: soPublicArgument}),
		makeTestEvent(&Event{UserID: uint32(user1.ID), Timestamp: startDate}),
		makeTestEvent(&Event{UserID: uint32(user1.ID), Timestamp: startDate}),

		makeTestEvent(&Event{UserID: uint32(sgAdmin.ID), Timestamp: secondDay}),
		makeTestEvent(&Event{UserID: uint32(user1.ID), Timestamp: secondDay}),
		makeTestEvent(&Event{UserID: uint32(user2.ID), Timestamp: secondDay}),
		makeTestEvent(&Event{UserID: uint32(sgAdmin.ID), Timestamp: secondDay}),
		makeTestEvent(&Event{UserID: uint32(soLoganID.ID), Timestamp: secondDay, PublicArgument: soPublicArgument}),
		makeTestEvent(&Event{UserID: uint32(soLoganID.ID), Timestamp: secondDay, PublicArgument: soPublicArgument}),

		makeTestEvent(&Event{UserID: uint32(user1.ID), Timestamp: thirdDay}),
		makeTestEvent(&Event{UserID: uint32(user2.ID), Timestamp: thirdDay}),
		makeTestEvent(&Event{UserID: uint32(user3.ID), Timestamp: thirdDay}),
		makeTestEvent(&Event{UserID: uint32(user4.ID), Timestamp: thirdDay}),
	}
	err = db.EventLogs().BulkInsert(ctx, events)
	require.NoError(t, err)

	values, err := db.EventLogs().SiteUsageMultiplePeriods(ctx, now, 3, 0, 0, nil)
	require.NoError(t, err)

	assertUsageValue(t, values.DAUs[0], startDate.Add(time.Hour*24*2), 4, 4, 0, 0)
	assertUsageValue(t, values.DAUs[1], startDate.Add(time.Hour*24), 4, 4, 0, 0)
	assertUsageValue(t, values.DAUs[2], startDate, 3, 3, 0, 0)

	values, err = db.EventLogs().SiteUsageMultiplePeriods(ctx, now, 3, 0, 0, &CountUniqueUsersOptions{CommonUsageOptions{ExcludeSourcegraphAdmins: true, ExcludeSourcegraphOperators: true}, nil})
	require.NoError(t, err)

	assertUsageValue(t, values.DAUs[0], startDate.Add(time.Hour*24*2), 4, 4, 0, 0)
	assertUsageValue(t, values.DAUs[1], startDate.Add(time.Hour*24), 2, 2, 0, 0)
	assertUsageValue(t, values.DAUs[2], startDate, 1, 1, 0, 0)
}

func TestEventLogs_UsersUsageCounts(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	now := time.Now()

	startDate, _ := calcStartDate(now, Daily, 3)
	secondDay := startDate.Add(time.Hour * 24)
	thirdDay := startDate.Add(time.Hour * 24 * 2)

	days := []time.Time{startDate, secondDay, thirdDay}
	names := []string{"SearchResultsQueried", "codeintel"}
	users := []uint32{1, 2}

	for _, day := range days {
		for _, user := range users {
			for _, name := range names {
				for i := 0; i < 25; i++ {
					e := &Event{
						UserID:    user,
						Name:      name,
						URL:       "http://sourcegraph.com",
						Source:    "test",
						Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60*12))),
					}

					if err := db.EventLogs().Insert(ctx, e); err != nil {
						t.Fatal(err)
					}
				}
			}
		}
	}

	have, err := db.EventLogs().UsersUsageCounts(ctx)
	if err != nil {
		t.Fatal(err)
	}

	want := []types.UserUsageCounts{
		{Date: days[2], UserID: users[0], SearchCount: 25, CodeIntelCount: 25},
		{Date: days[2], UserID: users[1], SearchCount: 25, CodeIntelCount: 25},
		{Date: days[1], UserID: users[0], SearchCount: 25, CodeIntelCount: 25},
		{Date: days[1], UserID: users[1], SearchCount: 25, CodeIntelCount: 25},
		{Date: days[0], UserID: users[0], SearchCount: 25, CodeIntelCount: 25},
		{Date: days[0], UserID: users[1], SearchCount: 25, CodeIntelCount: 25},
	}

	if diff := cmp.Diff(want, have); diff != "" {
		t.Error(diff)
	}
}

func TestEventLogs_SiteUsage(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	days := map[time.Time]struct {
		users   []uint32
		names   []string
		sources []string
	}{
		// Today
		now: {
			[]uint32{1, 2, 3, 4, 5},
			[]string{"ViewSiteAdminX"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// This week
		now.Add(-time.Hour * 24 * 3): {
			[]uint32{0, 2, 3, 5},
			[]string{"ViewRepository", "ViewTree"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// This week
		now.Add(-time.Hour * 24 * 4): {
			[]uint32{1, 3, 5, 7},
			[]string{"ViewSiteAdminX", "SavedSearchSlackClicked"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// This month
		now.Add(-time.Hour * 24 * 6): {
			[]uint32{0, 1, 8, 9},
			[]string{"ViewSiteAdminX"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// This month
		now.Add(-time.Hour * 24 * 12): {
			[]uint32{1, 2, 3, 4, 5, 6, 11},
			[]string{"ViewTree", "SavedSearchSlackClicked"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// Previous month
		now.Add(-time.Hour * 24 * 40): {
			[]uint32{0, 1, 5, 6, 13},
			[]string{"SearchResultsQueried", "DiffSearchResultsQueried"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
	}

	for day, data := range days {
		for _, user := range data.users {
			for _, name := range data.names {
				for _, source := range data.sources {
					for i := 0; i < 5; i++ {
						e := &Event{
							UserID: user,
							Name:   name,
							URL:    "http://sourcegraph.com",
							Source: source,
							// Jitter current time +/- 30 minutes
							Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60)-30)),
						}

						if user == 0 {
							e.AnonymousUserID = "deadbeef"
						}

						if err := db.EventLogs().Insert(ctx, e); err != nil {
							t.Fatal(err)
						}
					}
				}
			}
		}
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	summary, err := el.siteUsageCurrentPeriods(ctx, now, nil)
	if err != nil {
		t.Fatal(err)
	}

	expectedSummary := types.SiteUsageSummary{
		Month:                   time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
		Week:                    now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5), // the previous Sunday
		Day:                     now.Truncate(time.Hour * 24),
		UniquesMonth:            11,
		UniquesWeek:             7,
		UniquesDay:              5,
		RegisteredUniquesMonth:  10,
		RegisteredUniquesWeek:   6,
		RegisteredUniquesDay:    5,
		IntegrationUniquesMonth: 11,
		IntegrationUniquesWeek:  7,
		IntegrationUniquesDay:   5,
	}
	if diff := cmp.Diff(expectedSummary, summary); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_SiteUsage_ExcludeSourcegraphAdmins(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	// Several of the events will belong to Sourcegraph employee admin user and Sourcegraph Operator user account
	sgAdmin, err := db.Users().Create(ctx, NewUser{Username: "sourcegraph-admin"})
	require.NoError(t, err)
	err = db.UserEmails().Add(ctx, sgAdmin.ID, "admin@sourcegraph.com", nil)
	require.NoError(t, err)
	soLogan, err := db.UserExternalAccounts().CreateUserAndSave(
		ctx,
		NewUser{
			Username: "sourcegraph-operator-logan",
		},
		extsvc.AccountSpec{
			ServiceType: "sourcegraph-operator",
		},
		extsvc.AccountData{},
	)
	require.NoError(t, err)

	user1, err := db.Users().Create(ctx, NewUser{Username: "a"})
	require.NoError(t, err)
	user2, err := db.Users().Create(ctx, NewUser{Username: "b"})
	require.NoError(t, err)

	days := map[time.Time]struct {
		userIDs []uint32
		names   []string
		sources []string
	}{
		// Today
		now: {
			[]uint32{uint32(sgAdmin.ID)},
			[]string{"ViewSiteAdminX"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		now.Add(-time.Hour): {
			[]uint32{uint32(soLogan.ID)},
			[]string{"ViewSiteAdminX"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// This week
		now.Add(-time.Hour * 24 * 3): {
			[]uint32{uint32(sgAdmin.ID), uint32(user1.ID)},
			[]string{"ViewRepository", "ViewTree"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		now.Add(-time.Hour * 24 * 4): {
			[]uint32{uint32(soLogan.ID), uint32(user1.ID)},
			[]string{"ViewRepository", "ViewTree"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
		// This month
		now.Add(-time.Hour * 24 * 6): {
			[]uint32{uint32(user2.ID)},
			[]string{"ViewSiteAdminX", "SavedSearchSlackClicked"},
			[]string{"test", "CODEHOSTINTEGRATION"},
		},
	}

	for day, data := range days {
		for _, userID := range data.userIDs {
			for _, name := range data.names {
				for _, source := range data.sources {
					for i := 0; i < 5; i++ {
						e := &Event{
							UserID: userID,
							Name:   name,
							URL:    "http://sourcegraph.com",
							Source: source,
							// Jitter current time +/- 30 minutes
							Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60)-30)),
						}

						if userID == uint32(soLogan.ID) {
							e.PublicArgument = json.RawMessage(fmt.Sprintf(`{"%s": true}`, EventLogsSourcegraphOperatorKey))
						}

						err := db.EventLogs().Insert(ctx, e)
						require.NoError(t, err)
					}
				}
			}
		}
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	summary, err := el.siteUsageCurrentPeriods(ctx, now, &SiteUsageOptions{CommonUsageOptions{ExcludeSourcegraphAdmins: false}})
	require.NoError(t, err)

	expectedSummary := types.SiteUsageSummary{
		Month:                   time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
		Week:                    now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5), // the previous Sunday
		Day:                     now.Truncate(time.Hour * 24),
		UniquesMonth:            4,
		UniquesWeek:             3,
		UniquesDay:              2,
		RegisteredUniquesMonth:  4,
		RegisteredUniquesWeek:   3,
		RegisteredUniquesDay:    2,
		IntegrationUniquesMonth: 4,
		IntegrationUniquesWeek:  3,
		IntegrationUniquesDay:   2,
	}
	assert.Equal(t, expectedSummary, summary)

	summary, err = el.siteUsageCurrentPeriods(ctx, now, &SiteUsageOptions{CommonUsageOptions{ExcludeSourcegraphAdmins: true, ExcludeSourcegraphOperators: true}})
	require.NoError(t, err)

	expectedSummary = types.SiteUsageSummary{
		Month:                   time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
		Week:                    now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5), // the previous Sunday
		Day:                     now.Truncate(time.Hour * 24),
		UniquesMonth:            2,
		UniquesWeek:             1,
		UniquesDay:              0,
		RegisteredUniquesMonth:  2,
		RegisteredUniquesWeek:   1,
		RegisteredUniquesDay:    0,
		IntegrationUniquesMonth: 2,
		IntegrationUniquesWeek:  1,
		IntegrationUniquesDay:   0,
	}
	assert.Equal(t, expectedSummary, summary)
}

func TestEventLogs_codeIntelligenceWeeklyUsersCount(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	names := []string{"codeintel.lsifHover", "codeintel.searchReferences", "unknown event"}
	users1 := []uint32{10, 20, 30, 40, 50, 60, 70, 80}
	users2 := []uint32{15, 25, 35, 45, 55, 65, 75, 85}

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	for _, name := range names {
		for _, user := range users1 {
			e := &Event{
				UserID: user,
				Name:   name,
				URL:    "http://sourcegraph.com",
				Source: "test",
				// This week; jitter current time +/- 30 minutes
				Timestamp: now.Add(-time.Hour * 24 * 3).Add(time.Minute * time.Duration(rand.Intn(60)-30)),
			}

			if err := db.EventLogs().Insert(ctx, e); err != nil {
				t.Fatal(err)
			}
		}
		for _, user := range users2 {
			e := &Event{
				UserID: user,
				Name:   name,
				URL:    "http://sourcegraph.com",
				Source: "test",
				// This month: jitter current time +/- 30 minutes
				Timestamp: now.Add(-time.Hour * 24 * 12).Add(time.Minute * time.Duration(rand.Intn(60)-30)),
			}

			if err := db.EventLogs().Insert(ctx, e); err != nil {
				t.Fatal(err)
			}
		}
	}

	eventNames := []string{
		"codeintel.lsifHover",
		"codeintel.searchReferences",
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	count, err := el.codeIntelligenceWeeklyUsersCount(ctx, eventNames, now)
	if err != nil {
		t.Fatal(err)
	}

	if count != len(users1) {
		t.Errorf("unexpected count. want=%d have=%d", len(users1), count)
	}
}

func TestEventLogs_TestCodeIntelligenceRepositoryCounts(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()
	now := time.Now()

	repos := []struct {
		id        int
		name      string
		deletedAt *time.Time
	}{
		{1, "test01", nil}, // 2 weeks old
		{2, "test02", nil},
		{3, "test03", nil},
		{4, "test04", nil},  // (no LSIF data)
		{5, "test05", &now}, // deleted
	}
	for _, repo := range repos {
		query := sqlf.Sprintf(
			"INSERT INTO repo (id, name, deleted_at) VALUES (%s, %s, %s)",
			repo.id,
			repo.name,
			repo.deletedAt,
		)
		if _, err := db.Handle().ExecContext(ctx, query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
			t.Fatalf("unexpected error preparing database: %s", err.Error())
		}
	}

	uploads := []struct {
		repositoryID int
	}{
		{1},
		{1}, // duplicate
		{2},
		{3},
		{5}, // deleted repository
		{6}, // missing repository
	}

	// Insert each upload once a day; first two uploads are not fresh
	// Add an extra hour so that we're not testing the weird edge boundary
	// when Postgres NOW() - interval and the record's upload time is not
	// too close.
	uploadedAt := time.Now().UTC().Add(-time.Hour * 24 * (7 + 2)).Add(time.Hour)

	for i, upload := range uploads {
		query := sqlf.Sprintf(
			"INSERT INTO lsif_uploads (repository_id, commit, indexer, uploaded_at, num_parts, uploaded_parts, state) VALUES (%s, %s, 'idx', %s, 1, '{}', 'completed')",
			upload.repositoryID,
			fmt.Sprintf("%040d", i),
			uploadedAt,
		)
		if _, err := db.Handle().ExecContext(ctx, query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
			t.Fatalf("unexpected error preparing database: %s", err.Error())
		}

		uploadedAt = uploadedAt.Add(time.Hour * 24)
	}

	query := sqlf.Sprintf(
		"INSERT INTO lsif_index_configuration (repository_id, data, autoindex_enabled) VALUES (1, '', true)",
	)
	if _, err := db.Handle().ExecContext(ctx, query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
		t.Fatalf("unexpected error preparing database: %s", err.Error())
	}

	query = sqlf.Sprintf(
		`
		INSERT INTO lsif_indexes (repository_id, commit, indexer, root, indexer_args, outfile, local_steps, docker_steps, queued_at, state) VALUES
			(1, %s, 'idx', '', '{}', 'dump.lsif', '{}', '{}', %s, 'completed'),
			(2, %s, 'idx', '', '{}', 'dump.lsif', '{}', '{}', %s, 'completed'),
			(3, %s, 'idx', '', '{}', 'dump.lsif', '{}', '{}', NOW(), 'queued') -- ignored
		`,
		fmt.Sprintf("%040d", 1), time.Now().UTC().Add(-time.Hour*24*7*2), // 2 weeks
		fmt.Sprintf("%040d", 2), time.Now().UTC().Add(-time.Hour*24*5), // 5 days
		fmt.Sprintf("%040d", 3),
	)
	if _, err := db.Handle().ExecContext(ctx, query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
		t.Fatalf("unexpected error preparing database: %s", err.Error())
	}

	t.Run("All", func(t *testing.T) {
		counts, err := db.EventLogs().CodeIntelligenceRepositoryCounts(ctx)
		if err != nil {
			t.Fatal(err)
		}

		if counts.NumRepositories != 4 {
			t.Errorf("unexpected number of repositories. want=%d have=%d", 4, counts.NumRepositories)
		}
		if counts.NumRepositoriesWithUploadRecords != 3 {
			t.Errorf("unexpected number of repositories with uploads. want=%d have=%d", 3, counts.NumRepositoriesWithUploadRecords)
		}
		if counts.NumRepositoriesWithFreshUploadRecords != 2 {
			t.Errorf("unexpected number of repositories with fresh uploads. want=%d have=%d", 2, counts.NumRepositoriesWithFreshUploadRecords)
		}
		if counts.NumRepositoriesWithIndexRecords != 2 {
			t.Errorf("unexpected number of repositories with indexes. want=%d have=%d", 2, counts.NumRepositoriesWithIndexRecords)
		}
		if counts.NumRepositoriesWithFreshIndexRecords != 1 {
			t.Errorf("unexpected number of repositories with fresh indexes. want=%d have=%d", 1, counts.NumRepositoriesWithFreshIndexRecords)
		}
		if counts.NumRepositoriesWithAutoIndexConfigurationRecords != 1 {
			t.Errorf("unexpected number of repositories with index configuration. want=%d have=%d", 1, counts.NumRepositoriesWithAutoIndexConfigurationRecords)
		}
	})

	t.Run("ByLanguage", func(t *testing.T) {
		counts, err := db.EventLogs().CodeIntelligenceRepositoryCountsByLanguage(ctx)
		if err != nil {
			t.Fatal(err)
		}

		if len(counts) != 1 {
			t.Errorf("unexpected number of counts. want=%d have=%d", 1, len(counts))
		}

		for language, counts := range counts {
			if language != "idx" {
				t.Errorf("unexpected indexer. want=%s have=%s", "idx", language)
			}

			if counts.NumRepositoriesWithUploadRecords != 3 {
				t.Errorf("unexpected number of repositories with uploads. want=%d have=%d", 3, counts.NumRepositoriesWithUploadRecords)
			}
			if counts.NumRepositoriesWithFreshUploadRecords != 2 {
				t.Errorf("unexpected number of repositories with fresh uploads. want=%d have=%d", 2, counts.NumRepositoriesWithFreshUploadRecords)
			}
			if counts.NumRepositoriesWithIndexRecords != 2 {
				t.Errorf("unexpected number of repositories with indexes. want=%d have=%d", 2, counts.NumRepositoriesWithIndexRecords)
			}
			if counts.NumRepositoriesWithFreshIndexRecords != 1 {
				t.Errorf("unexpected number of repositories with fresh indexes. want=%d have=%d", 1, counts.NumRepositoriesWithFreshIndexRecords)
			}
		}
	})
}

func TestEventLogs_CodeIntelligenceSettingsPageViewCounts(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	names := []string{
		"ViewBatchesConfiguration",
		"ViewCodeIntelUploadsPage",       // contributes 75 events
		"ViewCodeIntelUploadPage",        // contributes 75 events
		"ViewCodeIntelIndexesPage",       // contributes 75 events
		"ViewCodeIntelIndexPage",         // contributes 75 events
		"ViewCodeIntelConfigurationPage", // contributes 75 events
	}

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	days := []time.Time{
		now,                           // Today
		now.Add(-time.Hour * 24 * 3),  // This week
		now.Add(-time.Hour * 24 * 4),  // This week
		now.Add(-time.Hour * 24 * 6),  // This month
		now.Add(-time.Hour * 24 * 12), // This month
		now.Add(-time.Hour * 24 * 40), // Previous month
	}

	g, gctx := errgroup.WithContext(ctx)

	for _, name := range names {
		for _, day := range days {
			for i := 0; i < 25; i++ {
				e := &Event{
					UserID:   1,
					Name:     name,
					URL:      "http://sourcegraph.com",
					Source:   "test",
					Argument: json.RawMessage(fmt.Sprintf(`{"languageId": "lang-%02d"}`, (i%3)+1)),
					// Jitter current time +/- 30 minutes
					Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60)-30)),
				}

				g.Go(func() error {
					return db.EventLogs().Insert(gctx, e)
				})
			}
		}
	}

	if err := g.Wait(); err != nil {
		t.Fatal(err)
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	count, err := el.codeIntelligenceSettingsPageViewCount(ctx, now)
	if err != nil {
		t.Fatal(err)
	}

	if count != 375 {
		t.Errorf("unexpected count. want=%d have=%d", 375, count)
	}
}

func TestEventLogs_AggregatedCodeIntelEvents(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	names := []string{"codeintel.lsifHover", "codeintel.searchReferences.xrepo", "unknown event"}
	users := []uint32{1, 2}

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	days := []time.Time{
		now,                           // Today
		now.Add(-time.Hour * 24 * 3),  // This week
		now.Add(-time.Hour * 24 * 4),  // This week
		now.Add(-time.Hour * 24 * 6),  // This month
		now.Add(-time.Hour * 24 * 12), // This month
		now.Add(-time.Hour * 24 * 40), // Previous month
	}

	g, gctx := errgroup.WithContext(ctx)

	for _, user := range users {
		for _, name := range names {
			for _, day := range days {
				for i := 0; i < 25; i++ {
					e := &Event{
						UserID:   user,
						Name:     name,
						URL:      "http://sourcegraph.com",
						Source:   "test",
						Argument: json.RawMessage(fmt.Sprintf(`{"languageId": "lang-%02d"}`, (i%3)+1)),
						// Jitter current time +/- 30 minutes
						Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60)-30)),
					}

					g.Go(func() error {
						return db.EventLogs().Insert(gctx, e)
					})
				}
			}
		}
	}

	if err := g.Wait(); err != nil {
		t.Fatal(err)
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	events, err := el.aggregatedCodeIntelEvents(ctx, now)
	if err != nil {
		t.Fatal(err)
	}

	lang1 := "lang-01"
	lang2 := "lang-02"
	lang3 := "lang-03"

	// the previous Sunday
	week := now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5)

	expectedEvents := []types.CodeIntelAggregatedEvent{
		{Name: "codeintel.lsifHover", LanguageID: &lang1, Week: week, TotalWeek: 54, UniquesWeek: 2},
		{Name: "codeintel.lsifHover", LanguageID: &lang2, Week: week, TotalWeek: 48, UniquesWeek: 2},
		{Name: "codeintel.lsifHover", LanguageID: &lang3, Week: week, TotalWeek: 48, UniquesWeek: 2},
		{Name: "codeintel.searchReferences.xrepo", LanguageID: &lang1, Week: week, TotalWeek: 54, UniquesWeek: 2},
		{Name: "codeintel.searchReferences.xrepo", LanguageID: &lang2, Week: week, TotalWeek: 48, UniquesWeek: 2},
		{Name: "codeintel.searchReferences.xrepo", LanguageID: &lang3, Week: week, TotalWeek: 48, UniquesWeek: 2},
	}
	if diff := cmp.Diff(expectedEvents, events); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_AggregatedSparseCodeIntelEvents(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	for i := 0; i < 5; i++ {
		e := &Event{
			UserID:    1,
			Name:      "codeintel.searchReferences.xrepo",
			URL:       "http://sourcegraph.com",
			Source:    "test",
			Argument:  json.RawMessage(fmt.Sprintf(`{"languageId": "lang-%02d"}`, (i%3)+1)),
			Timestamp: now.Add(-time.Hour * 24 * 3), // This week
		}

		if err := db.EventLogs().Insert(ctx, e); err != nil {
			t.Fatal(err)
		}
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	events, err := el.aggregatedCodeIntelEvents(ctx, now)
	if err != nil {
		t.Fatal(err)
	}

	lang1 := "lang-01"
	lang2 := "lang-02"
	lang3 := "lang-03"

	// the previous Sunday
	week := now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5)

	expectedEvents := []types.CodeIntelAggregatedEvent{
		{Name: "codeintel.searchReferences.xrepo", LanguageID: &lang1, Week: week, TotalWeek: 2, UniquesWeek: 1},
		{Name: "codeintel.searchReferences.xrepo", LanguageID: &lang2, Week: week, TotalWeek: 2, UniquesWeek: 1},
		{Name: "codeintel.searchReferences.xrepo", LanguageID: &lang3, Week: week, TotalWeek: 1, UniquesWeek: 1},
	}
	if diff := cmp.Diff(expectedEvents, events); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_AggregatedCodeIntelInvestigationEvents(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	names := []string{
		"CodeIntelligenceIndexerSetupInvestigated",
		"CodeIntelligenceIndexerSetupInvestigated", // duplicate
		"CodeIntelligenceUploadErrorInvestigated",
		"CodeIntelligenceIndexErrorInvestigated",
		"unknown event"}
	users := []uint32{1, 2}

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	days := []time.Time{
		now,                           // Today
		now.Add(-time.Hour * 24 * 3),  // This week
		now.Add(-time.Hour * 24 * 4),  // This week
		now.Add(-time.Hour * 24 * 6),  // This month
		now.Add(-time.Hour * 24 * 12), // This month
		now.Add(-time.Hour * 24 * 40), // Previous month
	}

	g, gctx := errgroup.WithContext(ctx)

	for _, user := range users {
		for _, name := range names {
			for _, day := range days {
				for i := 0; i < 25; i++ {
					e := &Event{
						UserID: user,
						Name:   name,
						URL:    "http://sourcegraph.com",
						Source: "test",
						// Jitter current time +/- 30 minutes
						Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60)-30)),
					}

					g.Go(func() error {
						return db.EventLogs().Insert(gctx, e)
					})
				}
			}
		}
	}

	if err := g.Wait(); err != nil {
		t.Fatal(err)
	}

	el := &eventLogStore{Store: basestore.NewWithHandle(db.Handle())}
	events, err := el.aggregatedCodeIntelInvestigationEvents(ctx, now)
	if err != nil {
		t.Fatal(err)
	}

	// the previous Sunday
	week := now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5)

	expectedEvents := []types.CodeIntelAggregatedInvestigationEvent{
		{Name: "CodeIntelligenceIndexErrorInvestigated", Week: week, TotalWeek: 150, UniquesWeek: 2},
		{Name: "CodeIntelligenceIndexerSetupInvestigated", Week: week, TotalWeek: 300, UniquesWeek: 2},
		{Name: "CodeIntelligenceUploadErrorInvestigated", Week: week, TotalWeek: 150, UniquesWeek: 2},
	}
	if diff := cmp.Diff(expectedEvents, events); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_AggregatedSparseSearchEvents(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	for i := 0; i < 5; i++ {
		e := &Event{
			UserID: 1,
			Name:   "search.latencies.structural",
			URL:    "http://sourcegraph.com",
			Source: "test",
			// Make durations non-uniform to test percent_cont. The values
			// in this test were hand-checked before being added to the assertion.
			// Adding additional events or changing parameters will require these
			// values to be checked again.
			Argument:  json.RawMessage(fmt.Sprintf(`{"durationMs": %d}`, 50)),
			Timestamp: now.Add(-time.Hour * 24 * 6), // This month
		}

		if err := db.EventLogs().Insert(ctx, e); err != nil {
			t.Fatal(err)
		}
	}

	events, err := db.EventLogs().AggregatedSearchEvents(ctx, now)
	if err != nil {
		t.Fatal(err)
	}

	expectedEvents := []types.SearchAggregatedEvent{
		{
			Name:           "search.latencies.structural",
			Month:          time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
			Week:           now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5), // the previous Sunday
			Day:            now.Truncate(time.Hour * 24),
			TotalMonth:     5,
			TotalWeek:      0,
			TotalDay:       0,
			UniquesMonth:   1,
			UniquesWeek:    0,
			UniquesDay:     0,
			LatenciesMonth: []float64{50, 50, 50},
			LatenciesWeek:  nil,
			LatenciesDay:   nil,
		},
	}
	if diff := cmp.Diff(expectedEvents, events); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_AggregatedSearchEvents(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	names := []string{"search.latencies.literal", "search.latencies.structural", "unknown event"}
	users := []uint32{1, 2}
	durations := []int{40, 65, 72}

	// This unix timestamp is equivalent to `Friday, May 15, 2020 10:30:00 PM GMT` and is set to
	// be a consistent value so that the tests don't fail when someone runs it at some particular
	// time that falls too near the edge of a week.
	now := time.Unix(1589581800, 0).UTC()

	days := []time.Time{
		now,                           // Today
		now.Add(-time.Hour * 24 * 3),  // This week
		now.Add(-time.Hour * 24 * 4),  // This week
		now.Add(-time.Hour * 24 * 6),  // This month
		now.Add(-time.Hour * 24 * 12), // This month
		now.Add(-time.Hour * 24 * 40), // Previous month
	}

	g, gctx := errgroup.WithContext(ctx)

	// add some latencies
	durationOffset := 0
	for _, user := range users {
		for _, name := range names {
			for _, duration := range durations {
				for _, day := range days {
					for i := 0; i < 25; i++ {
						durationOffset++

						e := &Event{
							UserID: user,
							Name:   name,
							URL:    "http://sourcegraph.com",
							Source: "test",
							// Make durations non-uniform to test percent_cont. The values
							// in this test were hand-checked before being added to the assertion.
							// Adding additional events or changing parameters will require these
							// values to be checked again.
							Argument: json.RawMessage(fmt.Sprintf(`{"durationMs": %d}`, duration+durationOffset)),
							// Jitter current time +/- 30 minutes
							Timestamp: day.Add(time.Minute * time.Duration(rand.Intn(60)-30)),
						}

						g.Go(func() error {
							return db.EventLogs().Insert(gctx, e)
						})
					}
				}
			}
		}
	}

	e := &Event{
		UserID: 3,
		Name:   "SearchResultsQueried",
		URL:    "http://sourcegraph.com",
		Source: "test",
		Argument: json.RawMessage(`
{
   "code_search":{
      "query_data":{
         "query":{
             "count_and":3,
             "count_repo_contains_commit_after":2,
             "count_repo_dependencies":5
         },
         "empty":false,
         "combined":"don't care"
      }
   }
}`),
		// Jitter current time +/- 30 minutes
		Timestamp: now.Add(-time.Hour * 24 * 3).Add(time.Minute * time.Duration(rand.Intn(60)-30)),
	}

	if err := db.EventLogs().Insert(gctx, e); err != nil {
		t.Fatal(err)
	}

	if err := g.Wait(); err != nil {
		t.Fatal(err)
	}

	events, err := db.EventLogs().AggregatedSearchEvents(ctx, now)
	if err != nil {
		t.Fatal(err)
	}

	expectedEvents := []types.SearchAggregatedEvent{
		{
			Name:           "search.latencies.literal",
			Month:          time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
			Week:           now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5), // the previous Sunday
			Day:            now.Truncate(time.Hour * 24),
			TotalMonth:     int32(len(users) * len(durations) * 25 * 5), // 5 days in month
			TotalWeek:      int32(len(users) * len(durations) * 25 * 3), // 3 days in week
			TotalDay:       int32(len(users) * len(durations) * 25),
			UniquesMonth:   2,
			UniquesWeek:    2,
			UniquesDay:     2,
			LatenciesMonth: []float64{944, 1772.1, 1839.51},
			LatenciesWeek:  []float64{919, 1752.1, 1792.51},
			LatenciesDay:   []float64{894, 1732.1, 1745.51},
		},
		{
			Name:           "search.latencies.structural",
			Month:          time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
			Week:           now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5), // the previous Sunday
			Day:            now.Truncate(time.Hour * 24),
			TotalMonth:     int32(len(users) * len(durations) * 25 * 5), // 5 days in month
			TotalWeek:      int32(len(users) * len(durations) * 25 * 3), // 3 days in week
			TotalDay:       int32(len(users) * len(durations) * 25),
			UniquesMonth:   2,
			UniquesWeek:    2,
			UniquesDay:     2,
			LatenciesMonth: []float64{1394, 2222.1, 2289.51},
			LatenciesWeek:  []float64{1369, 2202.1, 2242.51},
			LatenciesDay:   []float64{1344, 2182.1, 2195.51},
		},
		{
			Name:         "count_and",
			Month:        time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
			Week:         now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5),
			Day:          now.Truncate(time.Hour * 24),
			TotalMonth:   3,
			TotalWeek:    3,
			TotalDay:     0,
			UniquesMonth: 1,
			UniquesWeek:  1,
		},
		{
			Name:         "count_repo_contains_commit_after",
			Month:        time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
			Week:         now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5),
			Day:          now.Truncate(time.Hour * 24),
			TotalMonth:   2,
			TotalWeek:    2,
			TotalDay:     0,
			UniquesMonth: 1,
			UniquesWeek:  1,
		},
		{
			Name:         "count_repo_dependencies",
			Month:        time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC),
			Week:         now.Truncate(time.Hour * 24).Add(-time.Hour * 24 * 5),
			Day:          now.Truncate(time.Hour * 24),
			TotalMonth:   5,
			TotalWeek:    5,
			TotalDay:     0,
			UniquesMonth: 1,
			UniquesWeek:  1,
		},
	}
	if diff := cmp.Diff(expectedEvents, events); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_ListAll(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	now := time.Now()

	startDate, _ := calcStartDate(now, Daily, 3)

	events := []*Event{
		{
			UserID:    1,
			Name:      "SearchResultsQueried",
			URL:       "http://sourcegraph.com",
			Source:    "test",
			Timestamp: startDate,
		}, {
			UserID:    2,
			Name:      "codeintel",
			URL:       "http://sourcegraph.com",
			Source:    "test",
			Timestamp: startDate,
		},
		{
			UserID:    2,
			Name:      "ViewRepository",
			URL:       "http://sourcegraph.com",
			Source:    "test",
			Timestamp: startDate,
		},
		{
			UserID:    2,
			Name:      "SearchResultsQueried",
			URL:       "http://sourcegraph.com",
			Source:    "test",
			Timestamp: startDate,
		}}

	for _, event := range events {
		if err := db.EventLogs().Insert(ctx, event); err != nil {
			t.Fatal(err)
		}
	}

	searchResultQueriedEvent := "SearchResultsQueried"
	have, err := db.EventLogs().ListAll(ctx, EventLogsListOptions{EventName: &searchResultQueriedEvent})
	if err != nil {
		t.Fatal(err)
	}

	want := 2

	if diff := cmp.Diff(want, len(have)); diff != "" {
		t.Error(diff)
	}
}

func TestEventLogs_LatestPing(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))

	t.Run("with no pings in database", func(t *testing.T) {
		ctx := context.Background()
		ping, err := db.EventLogs().LatestPing(ctx)
		if ping != nil {
			t.Fatalf("have ping %+v, expected nil", ping)
		}
		if err != sql.ErrNoRows {
			t.Fatalf("have err %+v, expected no rows error", err)
		}
	})

	ptr := func(s string) *string {
		return &s
	}

	t.Run("with existing pings in database", func(t *testing.T) {
		userID := int32(0)
		timestamp := timeutil.Now()

		ctx := context.Background()
		events := []*Event{
			{
				UserID:          0,
				Name:            "ping",
				URL:             "http://sourcegraph.com",
				AnonymousUserID: "test",
				Source:          "test",
				Timestamp:       timestamp,
				Argument:        json.RawMessage(`{"key": "value1"}`),
				DeviceID:        ptr("device-id"),
				InsertID:        ptr("insert-id"),
			}, {
				UserID:          0,
				Name:            "ping",
				URL:             "http://sourcegraph.com",
				AnonymousUserID: "test",
				Source:          "test",
				Timestamp:       timestamp,
				Argument:        json.RawMessage(`{"key": "value2"}`),
				DeviceID:        ptr("device-id"),
				InsertID:        ptr("insert-id"),
			},
		}
		for _, event := range events {
			if err := db.EventLogs().Insert(ctx, event); err != nil {
				t.Fatal(err)
			}
		}

		gotPing, err := db.EventLogs().LatestPing(ctx)
		if err != nil || gotPing == nil {
			t.Fatal(err)
		}
		expectedPing := &Event{
			ID:              2,
			Name:            events[1].Name,
			URL:             events[1].URL,
			UserID:          uint32(userID),
			AnonymousUserID: events[1].AnonymousUserID,
			Version:         version.Version(),
			Argument:        events[1].Argument,
			Source:          events[1].Source,
			Timestamp:       timestamp,
		}
		expectedPing.DeviceID = ptr("device-id")
		expectedPing.InsertID = ptr("insert-id") // set these values for test determinism
		if diff := cmp.Diff(gotPing, expectedPing); diff != "" {
			t.Fatal(diff)
		}
	})
}

// makeTestEvent sets the required (uninteresting) fields that are required on insertion
// due to database constraints. This method will also add some sub-day jitter to the timestamp.
func makeTestEvent(e *Event) *Event {
	if e.UserID == 0 {
		e.UserID = 1
	}
	e.Name = "foo"
	e.URL = "http://sourcegraph.com"
	e.Source = "WEB"
	e.Timestamp = e.Timestamp.Add(time.Minute * time.Duration(rand.Intn(60*12)))
	return e
}

func assertUsageValue(t *testing.T, v *types.SiteActivityPeriod, start time.Time, userCount, registeredUserCount, anonymousUserCount, integrationUserCount int) {
	t.Helper()

	if v.StartTime != start {
		t.Errorf("got StartTime %q, want %q", v.StartTime, start)
	}
	if int(v.UserCount) != userCount {
		t.Errorf("got UserCount %d, want %d", v.UserCount, userCount)
	}
	if int(v.RegisteredUserCount) != registeredUserCount {
		t.Errorf("got RegisteredUserCount %d, want %d", v.RegisteredUserCount, registeredUserCount)
	}
	if int(v.AnonymousUserCount) != anonymousUserCount {
		t.Errorf("got AnonymousUserCount %d, want %d", v.AnonymousUserCount, anonymousUserCount)
	}
	if int(v.IntegrationUserCount) != integrationUserCount {
		t.Errorf("got IntegrationUserCount %d, want %d", v.IntegrationUserCount, integrationUserCount)
	}
}

func TestEventLogs_RequestsByLanguage(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	t.Parallel()
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	if _, err := db.Handle().ExecContext(ctx, `
		INSERT INTO codeintel_langugage_support_requests (language_id, user_id)
		VALUES
			('foo', 1),
			('bar', 1),
			('bar', 2),
			('bar', 3),
			('baz', 1),
			('baz', 2),
			('baz', 3),
			('baz', 4)
	`); err != nil {
		t.Fatal(err)
	}

	requests, err := db.EventLogs().RequestsByLanguage(ctx)
	if err != nil {
		t.Fatal(err)
	}

	expectedRequests := map[string]int{
		"foo": 1,
		"bar": 3,
		"baz": 4,
	}
	if diff := cmp.Diff(expectedRequests, requests); diff != "" {
		t.Fatal(diff)
	}
}

func TestEventLogs_IllegalPeriodType(t *testing.T) {
	t.Run("calcStartDate", func(t *testing.T) {
		_, err := calcStartDate(time.Now(), "hackerman", 3)
		if err == nil {
			t.Error("want err to not be nil")
		}
	})
	t.Run("calcEndDate", func(t *testing.T) {
		_, err := calcEndDate(time.Now(), "hackerman", 3)
		if err == nil {
			t.Error("want err to not be nil")
		}
	})
}
