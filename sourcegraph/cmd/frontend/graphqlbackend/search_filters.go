package graphqlbackend

import (
	"fmt"
	"path"
	"regexp"
	"strconv"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/inventory"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
)

// SearchFilters computes the filters to show a user based on results.
//
// Note: it currently live in graphqlbackend. However, once we have a non
// resolver based SearchResult type it can be extracted. It lives in its own
// file to make that more obvious. We already have the filter type extracted
// (streaming.Filter).
type SearchFilters struct {
	// Globbing is true if the user has enabled globbing support.
	Globbing bool

	filters streaming.Filters
}

// commonFileFilters are common filters used. It is used by SearchFilters to
// propose them if they match shown results.
var commonFileFilters = []struct {
	regexp      *lazyregexp.Regexp
	regexFilter string
	globFilter  string
}{
	// Exclude go tests
	{
		regexp:      lazyregexp.New(`_test\.go$`),
		regexFilter: `-file:_test\.go$`,
		globFilter:  `-file:**_test.go`,
	},
	// Exclude go vendor
	{
		regexp:      lazyregexp.New(`(^|/)vendor/`),
		regexFilter: `-file:(^|/)vendor/`,
		globFilter:  `-file:vendor/** -file:**/vendor/**`,
	},
	// Exclude node_modules
	{
		regexp:      lazyregexp.New(`(^|/)node_modules/`),
		regexFilter: `-file:(^|/)node_modules/`,
		globFilter:  `-file:node_modules/** -file:**/node_modules/**`,
	},
	// Exclude minified javascript
	{
		regexp:      lazyregexp.New(`\.min\.js$`),
		regexFilter: `-file:\.min\.js$`,
		globFilter:  `-file:**.min.js`,
	},
	// Exclude javascript maps
	{
		regexp:      lazyregexp.New(`\.js\.map$`),
		regexFilter: `-file:\.js\.map$`,
		globFilter:  `-file:**.js.map`,
	},
}

// Update internal state for the results in event.
func (s *SearchFilters) Update(event SearchEvent) {
	// Avoid work if nothing to observe.
	if len(event.Results) == 0 {
		return
	}

	// Initialize state on first call.
	if s.filters == nil {
		s.filters = make(streaming.Filters)
	}

	addRepoFilter := func(repo *RepositoryResolver, rev string, lineMatchCount int32) {
		uri := repo.Name()
		var filter string
		if s.Globbing {
			filter = fmt.Sprintf(`repo:%s`, uri)
		} else {
			filter = fmt.Sprintf(`repo:^%s$`, regexp.QuoteMeta(uri))
		}

		if rev != "" {
			// We don't need to quote rev. The only special characters we interpret
			// are @ and :, both of which are disallowed in git refs
			filter = filter + fmt.Sprintf(`@%s`, rev)
		}
		limitHit := event.Stats.Status.Get(repo.IDInt32())&search.RepoStatusLimitHit != 0
		s.filters.Add(filter, uri, lineMatchCount, limitHit, "repo")
	}

	addFileFilter := func(fileMatchPath string, lineMatchCount int32, limitHit bool) {
		for _, ff := range commonFileFilters {
			// use regexp to match file paths unconditionally, whether globbing is enabled or not,
			// since we have no native library call to match `**` for globs.
			if ff.regexp.MatchString(fileMatchPath) {
				if s.Globbing {
					s.filters.Add(ff.globFilter, ff.globFilter, lineMatchCount, limitHit, "file")
				} else {
					s.filters.Add(ff.regexFilter, ff.regexFilter, lineMatchCount, limitHit, "file")
				}
			}
		}
	}

	addLangFilter := func(fileMatchPath string, lineMatchCount int32, limitHit bool) {
		extensionToLanguageLookup := func(path string) string {
			language, _ := inventory.GetLanguageByFilename(path)
			return strings.ToLower(language)
		}
		if ext := path.Ext(fileMatchPath); ext != "" {
			language := extensionToLanguageLookup(fileMatchPath)
			if language != "" {
				if strings.Contains(language, " ") {
					language = strconv.Quote(language)
				}
				value := fmt.Sprintf(`lang:%s`, language)
				s.filters.Add(value, value, lineMatchCount, limitHit, "lang")
			}
		}
	}

	if event.Stats.ExcludedForks > 0 {
		s.filters.Add("fork:yes", "fork:yes", int32(event.Stats.ExcludedForks), event.Stats.IsLimitHit, "repo")
		s.filters.MarkImportant("fork:yes")
	}
	if event.Stats.ExcludedArchived > 0 {
		s.filters.Add("archived:yes", "archived:yes", int32(event.Stats.ExcludedArchived), event.Stats.IsLimitHit, "repo")
		s.filters.MarkImportant("archived:yes")
	}
	for _, result := range event.Results {
		if fm, ok := result.ToFileMatch(); ok {
			rev := ""
			if fm.InputRev != nil {
				rev = *fm.InputRev
			}
			lines := fm.ResultCount()
			addRepoFilter(fm.RepoResolver, rev, lines)
			addLangFilter(fm.path(), lines, fm.LimitHit())
			addFileFilter(fm.path(), lines, fm.LimitHit())

			if len(fm.symbols) > 0 {
				s.filters.Add("type:symbol", "type:symbol", 1, fm.LimitHit(), "symbol")
			}
		} else if r, ok := result.ToRepository(); ok {
			// It should be fine to leave this blank since revision specifiers
			// can only be used with the 'repo:' scope. In that case,
			// we shouldn't be getting any repositoy name matches back.
			addRepoFilter(r, "", 1)
		}
	}
}

// Compute returns an ordered slice of Filters to present to the user based on
// events passed to Next.
func (s *SearchFilters) Compute() []*streaming.Filter {
	return s.filters.Compute()
}
