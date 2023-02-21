package keyword

import (
	"testing"

	"github.com/hexops/autogold/v2"

	"github.com/sourcegraph/sourcegraph/internal/search/query"
)

func TestTransformPattern(t *testing.T) {
	patterns := []string{
		"K",
		"Means",
		"Clustering",
		"convert",
		"int",
		"to",
		"string",
		"finding",
		"time",
		"elapsed",
		"using",
		"a",
		"timer",
	}
	wantPatterns := []string{
		"k",
		"mean",
		"cluster",
		"convert",
		"int",
		"string",
		"find",
		"time",
		"elaps",
		"using",
		"timer",
	}

	gotPatterns := transformPatterns(patterns)
	autogold.Expect(wantPatterns).Equal(t, gotPatterns)
}

func TestQueryStringToKeywordQuery(t *testing.T) {
	tests := []struct {
		query        string
		wantQuery    autogold.Value
		wantPatterns autogold.Value
	}{
		{
			query:        "context:global abc",
			wantQuery:    autogold.Expect("count:99999999 type:file context:global abc"),
			wantPatterns: autogold.Expect([]string{"abc"}),
		},
		{
			query:        "abc def",
			wantQuery:    autogold.Expect("count:99999999 type:file (abc OR def)"),
			wantPatterns: autogold.Expect([]string{"abc", "def"}),
		},
		{
			query:        "context:global lang:Go how to unzip file",
			wantQuery:    autogold.Expect("count:99999999 type:file context:global lang:Go (unzip OR file)"),
			wantPatterns: autogold.Expect([]string{"unzip", "file"}),
		},
		{
			query:        "K MEANS CLUSTERING in python",
			wantQuery:    autogold.Expect("count:99999999 type:file lang:Python (k OR mean OR cluster)"),
			wantPatterns: autogold.Expect([]string{"k", "mean", "cluster"}),
		},
	}

	for _, tt := range tests {
		t.Run(tt.query, func(t *testing.T) {
			keywordQuery, err := queryStringToKeywordQuery(tt.query)
			if err != nil {
				t.Fatal(err)
			}
			if keywordQuery == nil {
				t.Fatal("keywordQuery == nil")
			}

			tt.wantPatterns.Equal(t, keywordQuery.patterns)
			tt.wantQuery.Equal(t, query.StringHuman(keywordQuery.query.ToParseTree()))
		})
	}
}
