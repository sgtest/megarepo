package search

import (
	"encoding/json"
	"sort"
	"testing"

	"github.com/hexops/autogold"

	"github.com/sourcegraph/sourcegraph/internal/search/query"

	zoekt "github.com/google/zoekt/query"
)

func TestQueryToZoektQuery(t *testing.T) {
	cases := []struct {
		Name     string
		Type     IndexedRequestType
		Pattern  *TextPatternInfo
		Features Features
		Query    string
	}{
		{
			Name: "substr",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "foo",
				IncludePatterns:              nil,
				ExcludePattern:               "",
				PathPatternsAreCaseSensitive: false,
			},
			Query: "foo case:no",
		},
		{
			Name: "symbol substr",
			Type: SymbolRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "foo",
				IncludePatterns:              nil,
				ExcludePattern:               "",
				PathPatternsAreCaseSensitive: false,
			},
			Query: "sym:foo case:no",
		},
		{
			Name: "regex",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "(foo).*?(bar)",
				IncludePatterns:              nil,
				ExcludePattern:               "",
				PathPatternsAreCaseSensitive: false,
			},
			Query: "(foo).*?(bar) case:no",
		},
		{
			Name: "path",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "foo",
				IncludePatterns:              []string{`\.go$`, `\.yaml$`},
				ExcludePattern:               `\bvendor\b`,
				PathPatternsAreCaseSensitive: false,
			},
			Query: `foo case:no f:\.go$ f:\.yaml$ -f:\bvendor\b`,
		},
		{
			Name: "case",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              true,
				Pattern:                      "foo",
				IncludePatterns:              []string{`\.go$`, `yaml`},
				ExcludePattern:               "",
				PathPatternsAreCaseSensitive: true,
			},
			Query: `foo case:yes f:\.go$ f:yaml`,
		},
		{
			Name: "casepath",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              true,
				Pattern:                      "foo",
				IncludePatterns:              []string{`\.go$`, `\.yaml$`},
				ExcludePattern:               `\bvendor\b`,
				PathPatternsAreCaseSensitive: true,
			},
			Query: `foo case:yes f:\.go$ f:\.yaml$ -f:\bvendor\b`,
		},
		{
			Name: "path matches only",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "test",
				IncludePatterns:              []string{},
				ExcludePattern:               ``,
				PathPatternsAreCaseSensitive: true,
				PatternMatchesContent:        false,
				PatternMatchesPath:           true,
			},
			Query: `f:test`,
		},
		{
			Name: "content matches only",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "test",
				IncludePatterns:              []string{},
				ExcludePattern:               ``,
				PathPatternsAreCaseSensitive: true,
				PatternMatchesContent:        true,
				PatternMatchesPath:           false,
			},
			Query: `c:test`,
		},
		{
			Name: "content and path matches 1",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "test",
				IncludePatterns:              []string{},
				ExcludePattern:               ``,
				PathPatternsAreCaseSensitive: true,
				PatternMatchesContent:        true,
				PatternMatchesPath:           true,
			},
			Query: `test`,
		},
		{
			Name: "content and path matches 2",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "test",
				IncludePatterns:              []string{},
				ExcludePattern:               ``,
				PathPatternsAreCaseSensitive: true,
				PatternMatchesContent:        false,
				PatternMatchesPath:           false,
			},
			Query: `test`,
		},
		{
			Name: "repos must include",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IsRegExp:                     true,
				Pattern:                      "foo",
				FilePatternsReposMustInclude: []string{`\.go$`, `\.yaml$`},
				FilePatternsReposMustExclude: []string{`\.java$`, `\.xml$`},
			},
			Query: `foo (type:repo file:\.go$) (type:repo file:\.yaml$) -(type:repo file:\.java$) -(type:repo file:\.xml$)`,
		},
		{
			Name: "TextPatternInfo.Languages is ignored",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IncludePatterns: []string{`\.go$`},
				Languages:       []string{"go"},
			},
			Query: `file:"\\.go(?m:$)"`,
		},
		{
			Name: "language gets passed as both file include and lang: predicate",
			Type: TextRequest,
			Pattern: &TextPatternInfo{
				IncludePatterns: []string{`\.go$`},
				Languages:       []string{"go"},
			},
			Features: Features{
				ContentBasedLangFilters: true,
			},
			Query: `file:"\\.go(?m:$)" lang:Go`,
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			q, err := zoekt.Parse(tt.Query)
			if err != nil {
				t.Fatalf("failed to parse %q: %v", tt.Query, err)
			}
			got, err := QueryToZoektQuery(tt.Pattern, &tt.Features, tt.Type)
			if err != nil {
				t.Fatal("queryToZoektQuery failed:", err)
			}
			if !queryEqual(got, q) {
				t.Fatalf("mismatched queries\ngot  %s\nwant %s", got.String(), q.String())
			}
		})
	}
}

func queryEqual(a, b zoekt.Q) bool {
	sortChildren := func(q zoekt.Q) zoekt.Q {
		switch s := q.(type) {
		case *zoekt.And:
			sort.Slice(s.Children, func(i, j int) bool {
				return s.Children[i].String() < s.Children[j].String()
			})
		case *zoekt.Or:
			sort.Slice(s.Children, func(i, j int) bool {
				return s.Children[i].String() < s.Children[j].String()
			})
		}
		return q
	}
	return zoekt.Map(a, sortChildren).String() == zoekt.Map(b, sortChildren).String()
}

func overrideSearchType(input string, searchType query.SearchType) query.SearchType {
	q, err := query.Parse(input, query.SearchTypeLiteral)
	q = query.LowercaseFieldNames(q)
	if err != nil {
		// If parsing fails, return the default search type. Any actual
		// parse errors will be raised by subsequent parser calls.
		return searchType
	}
	query.VisitField(q, "patterntype", func(value string, _ bool, _ query.Annotation) {
		switch value {
		case "regex", "regexp":
			searchType = query.SearchTypeRegex
		case "literal":
			searchType = query.SearchTypeLiteral
		case "structural":
			searchType = query.SearchTypeStructural
		}
	})
	return searchType
}

func TestToTextPatternInfo(t *testing.T) {
	test := func(input string) string {
		searchType := overrideSearchType(input, query.SearchTypeLiteral)
		plan, err := query.Pipeline(query.Init(input, searchType))
		if err != nil {
			return "Error"
		}
		if len(plan) == 0 {
			return "Empty"
		}
		q := plan[0]
		mode := Batch
		p := ToTextPatternInfo(q, mode, query.Identity)
		v, _ := json.Marshal(p)
		return string(v)
	}

	autogold.Want("01", `{"Pattern":"archived","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`type:repo archived`))

	autogold.Want("02", `{"Pattern":"archived","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`type:repo archived archived:yes`))

	autogold.Want("03", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/archived$`))

	autogold.Want("04", `{"Pattern":"sgtest/mux","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`type:repo sgtest/mux`))

	autogold.Want("05", `{"Pattern":"sgtest/mux","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`type:repo sgtest/mux fork:yes`))

	autogold.Want("06", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/mux$`))

	autogold.Want("07", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:github\.com/sgtest/mux fork:true`))

	autogold.Want("08", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:mux|archived|go-diff`))

	autogold.Want("09", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ patterntype:structural`))

	autogold.Want("10", `{"Pattern":"func main\\(\\) \\{\n","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`"func main() {\n" patterntype:regexp type:file`))

	autogold.Want("11", `{"Pattern":"func main\\(\\) \\{\n","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`"func main() {\n" -repo:go-diff patterntype:regexp type:file`))

	autogold.Want("12", `{"Pattern":"String","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":true,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":true,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ String case:yes type:file`))

	autogold.Want("13", `{"Pattern":"void sendPartialResult\\(Object requestId, JsonPatch jsonPatch\\);","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/java-langserver$@v1 void sendPartialResult(Object requestId, JsonPatch jsonPatch); patterntype:literal type:file`))

	autogold.Want("14", `{"Pattern":"void sendPartialResult\\(Object requestId, JsonPatch jsonPatch\\);","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":1,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/java-langserver$@v1 void sendPartialResult(Object requestId, JsonPatch jsonPatch); patterntype:literal count:1 type:file`))

	autogold.Want("15", `{"Pattern":"\\nimport","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"only","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/java-langserver$ \nimport index:only patterntype:regexp type:file`))

	autogold.Want("16", `{"Pattern":"\\nimport","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"no","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/java-langserver$ \nimport index:no patterntype:regexp type:file`))

	autogold.Want("17", `{"Pattern":"doesnot734734743734743exist","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/java-langserver$ doesnot734734743734743exist`))

	autogold.Want("18", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ type:commit`))

	autogold.Want("19", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$@ref/noexist type:commit`))

	autogold.Want("20", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ type:commit message:test`))

	autogold.Want("21", `{"Pattern":"test","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ type:commit test`))

	autogold.Want("22", `{"Pattern":"main","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ type:diff main`))

	autogold.Want("23", `{"Pattern":"test","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ repohascommitafter:"2019-01-01" test patterntype:literal`))

	autogold.Want("24", `{"Pattern":"^func.*$","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"only","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`^func.*$ patterntype:regexp index:only type:file`))

	autogold.Want("25", `{"Pattern":"FORK_SENTINEL","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`fork:only patterntype:regexp FORK_SENTINEL`))

	autogold.Want("26", `{"Pattern":"\\bfunc\\b","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["\\.go$"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":["go"]}`).Equal(t, test(`\bfunc\b lang:go type:file patterntype:regexp`))

	autogold.Want("27", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["asdfasdf.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`file:asdfasdf.go patterntype:regexp`))

	autogold.Want("28", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["doc.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`file:doc.go patterntype:regexp`))

	autogold.Want("29", `{"Pattern":"make(:[1])","IsNegated":false,"IsRegExp":false,"IsStructuralPat":true,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":3,"Index":"only","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ make(:[1]) index:only patterntype:structural count:3`))

	autogold.Want("30", `{"Pattern":"make(:[1])","IsNegated":false,"IsRegExp":false,"IsStructuralPat":true,"CombyRule":"where \"backcompat\" == \"backcompat\"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["\\.go$"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":["go"]}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ make(:[1]) lang:go rule:'where "backcompat" == "backcompat"' patterntype:structural`))

	autogold.Want("31", `{"Pattern":"make(:[1])","IsNegated":false,"IsRegExp":false,"IsStructuralPat":true,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":3,"Index":"no","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$@adde71 make(:[1]) index:no patterntype:structural count:3`))

	autogold.Want("32", `{"Pattern":"\"basic :[_] access :[_]\"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":true,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^README\\.md"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ file:^README\.md "basic :[_] access :[_]" patterntype:structural`))

	autogold.Want("33", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ patterntype:literal i can't :[believe] it's not butter`))

	autogold.Want("34", `{"Pattern":"no results for \\{ \\.\\.\\. \\} raises alert","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`no results for { ... } raises alert repo:^github\.com/sgtest/go-diff$`))

	autogold.Want("35", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ func and main type:file`))

	autogold.Want("36", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ func and main type:file`))

	autogold.Want("37", `{"Pattern":"func PrintMultiFileDiff","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ "func PrintMultiFileDiff" or 'func readLine(' type:file patterntype:regexp`))

	autogold.Want("38", `{"Pattern":"\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ (() or ()) type:file patterntype:regexp`))

	autogold.Want("39", `{"Pattern":"\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ () or () type:file patterntype:regexp`))

	autogold.Want("40", `{"Pattern":"\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ \(\) or \(\) type:file patterntype:regexp`))

	autogold.Want("41", `{"Pattern":"\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ () or \(\) type:file patterntype:regexp`))

	autogold.Want("42", `{"Pattern":"\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ (() or \(\)) type:file patterntype:regexp`))

	autogold.Want("43", `{"Pattern":"\\(\\)\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ ()() or ()()`))

	autogold.Want("44", `{"Pattern":"\\(\\)\\(\\)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ ()() or main()(`))

	autogold.Want("45", `{"Pattern":"\\(\\)\\(","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ ()( or ()()`))

	autogold.Want("46", `{"Pattern":"func(.*)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ patternType:regexp func(.*) or does_not_exist_3744 type:file`))

	autogold.Want("47", `{"Pattern":"func\\(","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ func( or func(.*) type:file`))

	autogold.Want("48", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ "*" and cert.*Load type:file`))

	autogold.Want("49", `{"Pattern":"(?:\\ and).*?(?:/)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ patternType:regexp \ and /`))

	autogold.Want("50", `{"Pattern":"t :=","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ file:^diff/print\.go t := or ts Time patterntype:literal`))

	autogold.Want("51", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go Bytes() and Time() patterntype:literal`))

	autogold.Want("52", `{"Pattern":"\\.svg","IsNegated":true,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ (not .svg) patterntype:literal`))

	autogold.Want("53", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ (a/foo not .svg) patterntype:literal`))

	autogold.Want("54", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ (a/foo and not .svg) patterntype:literal`))

	autogold.Want("55", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ content:"diffPath)" and main patterntype:literal`))

	autogold.Want("60", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^README\\.md"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^README\.md (bar and (foo or x\) ()) patterntype:literal`))

	autogold.Want("61", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^README\\.md"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^README\.md (bar and (foo or (x\) ())) patterntype:literal`))

	autogold.Want("62", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ (m *FileDiff and (data)) patterntype:literal`))

	autogold.Want("63", `{"Pattern":"(?:t).*?(?::=)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ file:^diff/print\.go t := or ts Time patterntype:regexp type:file`))

	autogold.Want("64", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ file:^diff/print\.go :[[v]] := ts and printFileHeader(:[_]) patterntype:structural`))

	autogold.Want("65", `{"Pattern":"func","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go func or package`))

	autogold.Want("66", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go func and package`))

	autogold.Want("67", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go ((func timePtr and package diff) or return buf.Bytes())`))

	autogold.Want("68", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go ((func timePtr and package diff) or (ts == nil and ts.Time()))`))

	autogold.Want("69", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go ((func timePtr or package diff) and (ts == nil or ts.Time()))`))

	autogold.Want("70", `{"Pattern":"","IsNegated":false,"IsRegExp":false,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^diff/print\\.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff file:^diff/print\.go func and doesnotexist838338`))

	autogold.Want("71", `{"Pattern":"_, :[[x]] := range :[src.] { :[_] }","IsNegated":false,"IsRegExp":false,"IsStructuralPat":true,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["diff.go|print.go|parse.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`file:diff.go|print.go|parse.go repo:^github\.com/sgtest/go-diff _, :[[x]] := range :[src.] { :[_] } or if :[s1] == :[s2] patterntype:structural`))

	autogold.Want("72", `{"Pattern":"Fetches","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ (Fetches OR file:language-server.ts)`))

	autogold.Want("73", `{"Pattern":"extends","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["^renovate\\.json"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ ((file:^renovate\.json extends) or file:progress.ts createProgressProvider)`))

	autogold.Want("74", `{"Pattern":"yarn","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ (type:diff or type:commit) author:felix yarn`))

	autogold.Want("75", `{"Pattern":"subscription","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ (type:diff or type:commit) subscription after:"june 11 2019" before:"june 13 2019"`))

	autogold.Want("76", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/mux$ (rev:v1.7.3 or revision:v1.7.2)`))

	autogold.Want("77", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["README.md"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/mux$ (rev:v1.7.3 or revision:v1.7.2) file:README.md`))

	autogold.Want("78", `{"Pattern":"#","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["README.md"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`(repo:^github\.com/sgtest/go-diff$@garo/lsif-indexing-campaign:test-already-exist-pr or repo:^github\.com/sgtest/sourcegraph-typescript$) file:README.md #`))

	autogold.Want("79", `{"Pattern":"package diff provides","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`(repo:^github\.com/sgtest/sourcegraph-typescript$ or repo:^github\.com/sgtest/go-diff$) package diff provides`))

	autogold.Want("80", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/go-diff$ type:commit (message:add or message:file)`))

	autogold.Want("81", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(file:go\.mod)`))

	autogold.Want("82", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(file:noexist.go)`))

	autogold.Want("83", `{"Pattern":"test","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(file:noexist.go) test`))

	autogold.Want("84", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(content:nextFileFirstLine)`))

	autogold.Want("86", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(content:does-not-exist-D2E1E74C7279) or repo:contains(content:nextFileFirstLine)`))

	autogold.Want("87", `{"Pattern":"fmt","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":100,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(file:go.mod) count:100 fmt`))

	autogold.Want("88", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff repo:contains(file:diff.proto)`))

	autogold.Want("89", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:nonexist repo:contains(file:diff.proto)`))

	autogold.Want("90", `{"Pattern":"LSIF","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`type:commit LSIF`))

	autogold.Want("91", `{"Pattern":"LSIF","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:contains(file:diff.pb.go) type:commit LSIF`))

	autogold.Want("92", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:sg(test)`))

	autogold.Want("93", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["repo"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal HunkNoChunksize select:repo`))

	autogold.Want("94", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["repo"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff select:repo`))

	autogold.Want("95", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["repo"],"IncludePatterns":["go-diff.go"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`file:go-diff.go select:repo`))

	autogold.Want("96", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["file"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal HunkNoChunksize select:file`))

	autogold.Want("97", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["file"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff HunkNoChunksize or ParseHunksAndPrintHunks select:file`))

	autogold.Want("98", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["content"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal HunkNoChunksize select:content`))

	autogold.Want("99", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal HunkNoChunksize`))

	autogold.Want("100", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["commit"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal HunkNoChunksize select:commit`))

	autogold.Want("101", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["symbol"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal HunkNoChunksize select:symbol`))

	autogold.Want("102", `{"Pattern":"HunkNoChunksize","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":["symbol"],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:go-diff patterntype:literal type:symbol HunkNoChunksize select:symbol`))

	autogold.Want("103", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":1000,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:^github\.com/sgtest/sourcegraph-typescript$ type:commit author:felix count:1000 before:"march 25 2021"`))

	autogold.Want("104", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["deploy"],"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`repo:sourcegraph-typescript$ type:file file:deploy`))

	autogold.Want("105", `{"Pattern":"(?:foo\\d).*?(?:bar\\*)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`foo\d "bar*" patterntype:regexp`))

	autogold.Want("106", `{"Pattern":"","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":["\\.go$"],"ExcludePattern":"(\\.java$)|(\\.jav$)","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":["go"]}`).Equal(t, test(`lang:go -lang:java`))

	autogold.Want("107", `{"Pattern":"(?://).*?(?:literal).*?(?:slash)","IsNegated":false,"IsRegExp":true,"IsStructuralPat":false,"CombyRule":"","IsWordMatch":false,"IsCaseSensitive":false,"FileMatchLimit":30,"Index":"yes","Select":[],"IncludePatterns":null,"ExcludePattern":"","FilePatternsReposMustInclude":null,"FilePatternsReposMustExclude":null,"PathPatternsAreCaseSensitive":false,"PatternMatchesContent":false,"PatternMatchesPath":false,"Languages":null}`).Equal(t, test(`patterntype:regexp // literal slash`))
}

func Test_toZoektPattern(t *testing.T) {
	test := func(input string) string {
		q, err := query.ParseLiteral(input)
		if err != nil {
			return err.Error()
		}
		b, err := query.ToBasicQuery(q)
		if err != nil {
			return err.Error()
		}
		zoektQuery, err := toZoektPattern(b.Pattern, false, true, true)
		if err != nil {
			return err.Error()
		}
		return zoektQuery.String()
	}

	autogold.Want("basic string",
		`file_substr:"a"`).
		Equal(t, test(`a`))

	autogold.Want("basic and-expression",
		`(or (and file_substr:"a" file_substr:"b" (not file_substr:"c")) file_substr:"d")`).
		Equal(t, test(`a and b and not c or d`))
}
