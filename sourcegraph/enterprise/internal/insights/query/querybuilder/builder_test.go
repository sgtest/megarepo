package querybuilder

import (
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/hexops/autogold"

	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestWithDefaults(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		want     string
		defaults query.Parameters
	}{
		{
			name:     "no defaults",
			input:    "repo:myrepo testquery",
			want:     "repo:myrepo testquery",
			defaults: []query.Parameter{},
		},
		{
			name:     "no defaults with fork archived",
			input:    "repo:myrepo testquery fork:no archived:no",
			want:     "repo:myrepo fork:no archived:no testquery",
			defaults: []query.Parameter{},
		},
		{
			name:     "no defaults with patterntype",
			input:    "repo:myrepo testquery patterntype:standard",
			want:     "repo:myrepo patterntype:standard testquery",
			defaults: []query.Parameter{},
		},
		{
			name:  "default archived",
			input: "repo:myrepo testquery fork:no",
			want:  "archived:yes repo:myrepo fork:no testquery",
			defaults: []query.Parameter{{
				Field:      query.FieldArchived,
				Value:      string(query.Yes),
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
		{
			name:  "default fork and archived",
			input: "repo:myrepo testquery",
			want:  "archived:no fork:no repo:myrepo testquery",
			defaults: []query.Parameter{{
				Field:      query.FieldArchived,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldFork,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
		{
			name:  "default patterntype",
			input: "repo:myrepo testquery",
			want:  "patterntype:literal repo:myrepo testquery",
			defaults: []query.Parameter{{
				Field:      query.FieldPatternType,
				Value:      "literal",
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
		{
			name:  "default patterntype does not override",
			input: "patterntype:standard repo:myrepo testquery",
			want:  "patterntype:standard repo:myrepo testquery",
			defaults: []query.Parameter{{
				Field:      query.FieldPatternType,
				Value:      "literal",
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := withDefaults(BasicQuery(test.input), test.defaults)
			if err != nil {
				t.Fatal(err)
			}
			if diff := cmp.Diff(test.want, string(got)); diff != "" {
				t.Fatalf("%s failed (want/got): %s", test.name, diff)
			}
		})
	}
}

func TestWithDefaultsPatternTypes(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		want     string
		defaults query.Parameters
	}{
		{
			// It's worth noting that we always append patterntype:regexp to capture group queries.
			name:     "regexp query without patterntype",
			input:    `file:go\.mod$ go\s*(\d\.\d+)`,
			want:     `file:go\.mod$ go\s*(\d\.\d+)`,
			defaults: []query.Parameter{},
		},
		{
			name:     "regexp query with patterntype",
			input:    `file:go\.mod$ go\s*(\d\.\d+) patterntype:regexp`,
			want:     `file:go\.mod$ patterntype:regexp go\s*(\d\.\d+)`,
			defaults: []query.Parameter{},
		},
		{
			name:     "literal query without patterntype",
			input:    `package search`,
			want:     `package search`,
			defaults: []query.Parameter{},
		},
		{
			name:     "literal query with patterntype",
			input:    `package search patterntype:literal`,
			want:     `patterntype:literal package search`,
			defaults: []query.Parameter{},
		},
		{
			name:     "literal query with quotes without patterntype",
			input:    `"license": "A`,
			want:     `"license": "A`,
			defaults: []query.Parameter{},
		},
		{
			name:     "literal query with quotes with patterntype",
			input:    `"license": "A patterntype:literal`,
			want:     `patterntype:literal "license": "A`,
			defaults: []query.Parameter{},
		},
		{
			name:     "structural query without patterntype",
			input:    `TODO(...)`,
			want:     `TODO(...)`,
			defaults: []query.Parameter{},
		},
		{
			name:     "structural query with patterntype",
			input:    `TODO(...) patterntype:structural`,
			want:     `patterntype:structural TODO(...)`,
			defaults: []query.Parameter{},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := withDefaults(BasicQuery(test.input), test.defaults)
			if err != nil {
				t.Fatal(err)
			}
			if diff := cmp.Diff(test.want, string(got)); diff != "" {
				t.Fatalf("%s failed (want/got): %s", test.name, diff)
			}
		})
	}
}

func TestMultiRepoQuery(t *testing.T) {
	tests := []struct {
		name     string
		repos    []string
		want     string
		defaults query.Parameters
	}{
		{
			name:     "single repo",
			repos:    []string{"repo1"},
			want:     `count:99999999 testquery repo:^(repo1)$`,
			defaults: []query.Parameter{},
		},
		{
			name:  "multiple repo",
			repos: []string{"repo1", "repo2"},
			want:  `archived:no fork:no count:99999999 testquery repo:^(repo1|repo2)$`,
			defaults: []query.Parameter{{
				Field:      query.FieldArchived,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldFork,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
		{
			name:  "multiple repo",
			repos: []string{"github.com/myrepos/repo1", "github.com/myrepos/repo2"},
			want:  `archived:no fork:no count:99999999 testquery repo:^(github\.com/myrepos/repo1|github\.com/myrepos/repo2)$`,
			defaults: []query.Parameter{{
				Field:      query.FieldArchived,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldFork,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := MultiRepoQuery("testquery", test.repos, test.defaults)
			if err != nil {
				t.Fatal(err)
			}
			if diff := cmp.Diff(test.want, string(got)); diff != "" {
				t.Fatalf("%s failed (want/got): %s", test.name, diff)
			}
		})
	}
}

func TestDefaults(t *testing.T) {
	tests := []struct {
		name  string
		input bool
		want  query.Parameters
	}{
		{
			name:  "all repos",
			input: true,
			want: query.Parameters{{
				Field:      query.FieldFork,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldArchived,
				Value:      string(query.No),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldPatternType,
				Value:      "literal",
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
		{
			name:  "some repos",
			input: false,
			want: query.Parameters{{
				Field:      query.FieldFork,
				Value:      string(query.Yes),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldArchived,
				Value:      string(query.Yes),
				Negated:    false,
				Annotation: query.Annotation{},
			}, {
				Field:      query.FieldPatternType,
				Value:      "literal",
				Negated:    false,
				Annotation: query.Annotation{},
			}},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got := CodeInsightsQueryDefaults(test.input)

			if diff := cmp.Diff(test.want, got); diff != "" {
				t.Fatalf("%s failed (want/got): %s", test.name, diff)
			}
		})
	}
}

func TestComputeInsightCommandQuery(t *testing.T) {
	tests := []struct {
		name       string
		inputQuery string
		mapType    MapType
		want       string
	}{
		{
			name:       "verify archive fork map to lang",
			inputQuery: "repo:abc123@12346f fork:yes archived:yes findme",
			mapType:    Lang,
			want:       "repo:abc123@12346f fork:yes archived:yes content:output.extra(findme -> $lang)",
		}, {
			name:       "verify archive fork map to repo",
			inputQuery: "repo:abc123@12346f fork:yes archived:yes findme",
			mapType:    Repo,
			want:       "repo:abc123@12346f fork:yes archived:yes content:output.extra(findme -> $repo)",
		}, {
			name:       "verify archive fork map to path",
			inputQuery: "repo:abc123@12346f fork:yes archived:yes findme",
			mapType:    Path,
			want:       "repo:abc123@12346f fork:yes archived:yes content:output.extra(findme -> $path)",
		}, {
			name:       "verify archive fork map to author",
			inputQuery: "repo:abc123@12346f fork:yes archived:yes findme",
			mapType:    Author,
			want:       "repo:abc123@12346f fork:yes archived:yes content:output.extra(findme -> $author)",
		}, {
			name:       "verify archive fork map to date",
			inputQuery: "repo:abc123@12346f fork:yes archived:yes findme",
			mapType:    Date,
			want:       "repo:abc123@12346f fork:yes archived:yes content:output.extra(findme -> $date)",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := ComputeInsightCommandQuery(BasicQuery(test.inputQuery), test.mapType)
			if err != nil {
				t.Error(err)
			}
			if diff := cmp.Diff(test.want, string(got)); diff != "" {
				t.Errorf("%s failed (want/got): %s", test.name, diff)
			}
		})
	}
}

func TestIsSingleRepoQuery(t *testing.T) {

	tests := []struct {
		name       string
		inputQuery string
		mapType    MapType
		want       bool
	}{
		{
			name:       "repo as simple text string",
			inputQuery: "repo:abc123@12346f fork:yes archived:yes findme",
			mapType:    Lang,
			want:       false,
		},
		{
			name:       "repo contains",
			inputQuery: "repo:contains.file(CHANGELOG) TEST",
			mapType:    Lang,
			want:       false,
		},
		{
			name:       "repo or",
			inputQuery: "repo:^(repo1|repo2)$ test",
			mapType:    Lang,
			want:       false,
		},
		{
			name:       "single repo with revision specified",
			inputQuery: `repo:^github\.com/sgtest/java-langserver$@v1 test`,
			mapType:    Lang,
			want:       true,
		},
		{
			name:       "single repo",
			inputQuery: `repo:^github\.com/sgtest/java-langserver$ test`,
			mapType:    Lang,
			want:       true,
		},
		{
			name:       "query without repo filter",
			inputQuery: `test`,
			mapType:    Lang,
			want:       false,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := IsSingleRepoQuery(BasicQuery(test.inputQuery))
			if err != nil {
				t.Error(err)
			}
			if diff := cmp.Diff(test.want, got); diff != "" {
				t.Errorf("%s failed (want/got): %s", test.name, diff)
			}

		})
	}
}

func TestIsSingleRepoQueryMultipleSteps(t *testing.T) {

	tests := []struct {
		name       string
		inputQuery string
		mapType    MapType
		want       error
	}{
		{
			name:       "2 step query different repos",
			inputQuery: `(repo:^github\.com/sourcegraph/sourcegraph$ OR repo:^github\.com/sourcegraph-testing/zap$) test`,
			mapType:    Lang,
			want:       QueryNotSupported,
		},
		{
			name:       "2 step query same repo",
			inputQuery: `(repo:^github\.com/sourcegraph/sourcegraph$ test) OR (repo:^github\.com/sourcegraph/sourcegraph$ todo)`,
			mapType:    Lang,
			want:       QueryNotSupported,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := IsSingleRepoQuery(BasicQuery(test.inputQuery))
			if !errors.Is(err, test.want) {
				t.Error(err)
			}
			if diff := cmp.Diff(false, got); diff != "" {
				t.Errorf("%s failed (want/got): %s", test.name, diff)
			}

		})
	}
}

func TestAggregationQuery(t *testing.T) {

	tests := []struct {
		name       string
		inputQuery string
		want       autogold.Value
	}{
		{
			inputQuery: `test`,
			want:       autogold.Want("basic query", BasicQuery("count:all timeout:2s test")),
		},
		{
			inputQuery: `(repo:^github\.com/sourcegraph/sourcegraph$ test) OR (repo:^github\.com/sourcegraph/sourcegraph$ todo)`,
			want:       autogold.Want("multiplan query", BasicQuery("(repo:^github\\.com/sourcegraph/sourcegraph$ count:all timeout:2s test OR repo:^github\\.com/sourcegraph/sourcegraph$ count:all timeout:2s todo)")),
		},
		{
			inputQuery: `(repo:^github\.com/sourcegraph/sourcegraph$ test) OR (repo:^github\.com/sourcegraph/sourcegraph$ todo) count:2000`,
			want:       autogold.Want("multiplan query overwrite", BasicQuery("(repo:^github\\.com/sourcegraph/sourcegraph$ count:all timeout:2s test OR repo:^github\\.com/sourcegraph/sourcegraph$ count:all timeout:2s todo)")),
		},
		{
			inputQuery: `test count:1000`,
			want:       autogold.Want("overwrite existing", BasicQuery("count:all timeout:2s test")),
		},
	}
	for _, test := range tests {
		t.Run(test.want.Name(), func(t *testing.T) {
			got, _ := AggregationQuery(BasicQuery(test.inputQuery), 2)
			test.want.Equal(t, got)

		})
	}
}
