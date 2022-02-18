package alert

import (
	"context"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/commit"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/run"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestAlertForDiffCommitSearchLimits(t *testing.T) {
	cases := []struct {
		name                 string
		multiErr             error
		wantAlertDescription string
	}{
		{
			name:                 "diff_search_warns_on_repos_greater_than_search_limit",
			multiErr:             errors.Append(nil, &commit.RepoLimitError{ResultType: "diff", Max: 50}),
			wantAlertDescription: `Diff search can currently only handle searching across 50 repositories at a time. Try using the "repo:" filter to narrow down which repositories to search, or using 'after:"1 week ago"'.`,
		},
		{
			name:                 "commit_search_warns_on_repos_greater_than_search_limit",
			multiErr:             errors.Append(nil, &commit.RepoLimitError{ResultType: "commit", Max: 50}),
			wantAlertDescription: `Commit search can currently only handle searching across 50 repositories at a time. Try using the "repo:" filter to narrow down which repositories to search, or using 'after:"1 week ago"'.`,
		},
		{
			name:                 "commit_search_warns_on_repos_greater_than_search_limit_with_time_filter",
			multiErr:             errors.Append(nil, &commit.TimeLimitError{ResultType: "commit", Max: 10000}),
			wantAlertDescription: `Commit search can currently only handle searching across 10000 repositories at a time. Try using the "repo:" filter to narrow down which repositories to search.`,
		},
	}

	for _, test := range cases {
		alert, _ := (&Observer{}).errorToAlert(context.Background(), test.multiErr)
		haveAlertDescription := alert.Description
		if diff := cmp.Diff(test.wantAlertDescription, haveAlertDescription); diff != "" {
			t.Fatalf("test %s, mismatched alert (-want, +got):\n%s", test.name, diff)
		}
	}
}

func TestErrorToAlertStructuralSearch(t *testing.T) {
	cases := []struct {
		name           string
		errors         []error
		wantAlertTitle string
	}{
		{
			name:           "multierr_is_unaffected",
			errors:         []error{errors.New("some error")},
			wantAlertTitle: "",
		},
		{
			name: "surface_friendly_alert_on_oom_err_message",
			errors: []error{
				errors.New("some error"),
				errors.New("Worker_oomed"),
				errors.New("some other error"),
			},
			wantAlertTitle: "Structural search needs more memory",
		},
	}
	for _, test := range cases {
		multiErr := errors.Append(nil, test.errors...)
		haveAlert, _ := (&Observer{}).errorToAlert(context.Background(), multiErr)

		if haveAlert != nil && haveAlert.Title != test.wantAlertTitle {
			t.Fatalf("test %s, have alert: %q, want: %q", test.name, haveAlert.Title, test.wantAlertTitle)
		}

	}
}

func TestAlertForNoResolvedReposWithNonGlobalSearchContext(t *testing.T) {
	db := database.NewDB(nil)

	searchQuery := "context:@user repo:r1 foo"
	wantAlert := &search.Alert{
		PrometheusType: "no_resolved_repos__context_none_in_common",
		Title:          "No repositories found for your query within the context @user",
		ProposedQueries: []*search.ProposedQuery{
			{

				Description: "search in the global context",
				Query:       "context:global repo:r1 foo",
				PatternType: query.SearchTypeRegex,
			},
		},
	}

	q, err := query.ParseLiteral(searchQuery)
	if err != nil {
		t.Fatal(err)
	}
	sr := Observer{
		Db: database.NewDB(db),
		SearchInputs: &run.SearchInputs{
			OriginalQuery: searchQuery,
			Query:         q,
			UserSettings:  &schema.Settings{},
		},
	}

	alert := sr.alertForNoResolvedRepos(context.Background(), q)
	require.NoError(t, err)
	require.Equal(t, wantAlert, alert)
}

func TestIsContextError(t *testing.T) {
	cases := []struct {
		err  error
		want bool
	}{
		{
			context.Canceled,
			true,
		},
		{
			context.DeadlineExceeded,
			true,
		},
		{
			errors.Wrap(context.Canceled, "wrapped"),
			true,
		},
		{
			errors.New("not a context error"),
			false,
		},
	}
	ctx := context.Background()
	for _, c := range cases {
		t.Run(c.err.Error(), func(t *testing.T) {
			if got := isContextError(ctx, c.err); got != c.want {
				t.Fatalf("wanted %t, got %t", c.want, got)
			}
		})
	}
}
