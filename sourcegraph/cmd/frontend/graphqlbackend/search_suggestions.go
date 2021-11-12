package graphqlbackend

import (
	"context"
	"math"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/cockroachdb/errors"
	"github.com/inconshreveable/log15"
	"github.com/neelance/parallel"
	"github.com/sourcegraph/go-lsp"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/searchcontexts"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

const maxSearchSuggestions = 100

// SearchSuggestionResolver is a resolver for the GraphQL union type `SearchSuggestion`
type SearchSuggestionResolver interface {
	// Score defines how well this item matches the query for sorting purposes
	Score() int

	// Length holds the length of the item name as a second sorting criterium
	Length() int

	// Label to sort alphabetically by when all else is equal.
	Label() string

	// Key is a key used to deduplicate suggestion results
	Key() suggestionKey

	ToRepository() (*RepositoryResolver, bool)
	ToFile() (*GitTreeEntryResolver, bool)
	ToGitBlob() (*GitTreeEntryResolver, bool)
	ToGitTree() (*GitTreeEntryResolver, bool)
	ToSymbol() (*symbolResolver, bool)
	ToLanguage() (*languageResolver, bool)
	ToSearchContext() (SearchContextResolver, bool)
}

// baseSuggestionResolver implements all the To* methods, returning false for all of them.
// Its intent is to be embedded into other suggestion resolvers to simplify implementing
// searchSuggestionResolver.
type baseSuggestionResolver struct{}

func (baseSuggestionResolver) ToRepository() (*RepositoryResolver, bool)      { return nil, false }
func (baseSuggestionResolver) ToFile() (*GitTreeEntryResolver, bool)          { return nil, false }
func (baseSuggestionResolver) ToGitBlob() (*GitTreeEntryResolver, bool)       { return nil, false }
func (baseSuggestionResolver) ToGitTree() (*GitTreeEntryResolver, bool)       { return nil, false }
func (baseSuggestionResolver) ToSymbol() (*symbolResolver, bool)              { return &symbolResolver{}, false }
func (baseSuggestionResolver) ToLanguage() (*languageResolver, bool)          { return nil, false }
func (baseSuggestionResolver) ToSearchContext() (SearchContextResolver, bool) { return nil, false }

// repositorySuggestionResolver implements searchSuggestionResolver for RepositoryResolver
type repositorySuggestionResolver struct {
	baseSuggestionResolver
	repo  *RepositoryResolver
	score int
}

func (r repositorySuggestionResolver) Score() int                                { return r.score }
func (r repositorySuggestionResolver) Length() int                               { return len(r.repo.Name()) }
func (r repositorySuggestionResolver) Label() string                             { return r.repo.Name() }
func (r repositorySuggestionResolver) ToRepository() (*RepositoryResolver, bool) { return r.repo, true }
func (r repositorySuggestionResolver) Key() suggestionKey {
	return suggestionKey{repoName: r.repo.Name()}
}

// gitTreeSuggestionResolver implements searchSuggestionResolver for GitTreeEntryResolver
type gitTreeSuggestionResolver struct {
	baseSuggestionResolver
	gitTreeEntry *GitTreeEntryResolver
	score        int
}

func (g gitTreeSuggestionResolver) Score() int    { return g.score }
func (g gitTreeSuggestionResolver) Length() int   { return len(g.gitTreeEntry.Path()) }
func (g gitTreeSuggestionResolver) Label() string { return g.gitTreeEntry.Path() }
func (g gitTreeSuggestionResolver) ToFile() (*GitTreeEntryResolver, bool) {
	return g.gitTreeEntry, true
}

func (g gitTreeSuggestionResolver) ToGitBlob() (*GitTreeEntryResolver, bool) {
	return g.gitTreeEntry, g.gitTreeEntry.stat.Mode().IsRegular()
}

func (g gitTreeSuggestionResolver) ToGitTree() (*GitTreeEntryResolver, bool) {
	return g.gitTreeEntry, g.gitTreeEntry.stat.Mode().IsDir()
}

func (g gitTreeSuggestionResolver) Key() suggestionKey {
	return suggestionKey{
		repoName: g.gitTreeEntry.Commit().Repository().Name(),
		repoRev:  string(g.gitTreeEntry.Commit().OID()),
		file:     g.gitTreeEntry.Path(),
	}
}

// symbolSuggestionResolver implements searchSuggestionResolver for symbolResolver
type symbolSuggestionResolver struct {
	baseSuggestionResolver
	symbol symbolResolver
	score  int
}

func (s symbolSuggestionResolver) Score() int { return s.score }
func (s symbolSuggestionResolver) Length() int {
	return len(s.symbol.Symbol.Name) + len(s.symbol.Symbol.Parent)
}

func (s symbolSuggestionResolver) Label() string {
	return s.symbol.Symbol.Name + " " + s.symbol.Symbol.Parent
}
func (s symbolSuggestionResolver) ToSymbol() (*symbolResolver, bool) { return &s.symbol, true }
func (s symbolSuggestionResolver) Key() suggestionKey {
	return suggestionKey{
		symbol: s.symbol.Symbol.Name + s.symbol.Symbol.Parent,
		url:    s.symbol.CanonicalURL(),
	}
}

// languageSuggestionResolver implements searchSuggestionResolver for languageResolver
type languageSuggestionResolver struct {
	baseSuggestionResolver
	lang  *languageResolver
	score int
}

func (l languageSuggestionResolver) Score() int                            { return l.score }
func (l languageSuggestionResolver) Length() int                           { return len(l.lang.Name()) }
func (l languageSuggestionResolver) Label() string                         { return l.lang.Name() }
func (l languageSuggestionResolver) ToLanguage() (*languageResolver, bool) { return l.lang, true }
func (l languageSuggestionResolver) Key() suggestionKey {
	return suggestionKey{
		lang: l.lang.Name(),
	}
}

func sortSearchSuggestions(s []SearchSuggestionResolver) {
	sort.Slice(s, func(i, j int) bool {
		// Sort by score
		a, b := s[i], s[j]
		if a.Score() != b.Score() {
			return a.Score() > b.Score()
		}
		// Prefer shorter strings for the same match score
		// E.g. prefer gorilla/mux over gorilla/muxy, Microsoft/vscode over g3ortega/vscode-crystal
		if a.Length() != b.Length() {
			return a.Length() < b.Length()
		}

		// All else equal, sort alphabetically.
		return a.Label() < b.Label()
	})
}

type searchContextSuggestionResolver struct {
	baseSuggestionResolver
	searchContext SearchContextResolver
	score         int
}

func (s searchContextSuggestionResolver) Score() int    { return s.score }
func (s searchContextSuggestionResolver) Length() int   { return len(s.searchContext.Spec()) }
func (s searchContextSuggestionResolver) Label() string { return s.searchContext.Spec() }
func (s searchContextSuggestionResolver) ToSearchContext() (SearchContextResolver, bool) {
	return s.searchContext, true
}

func (s searchContextSuggestionResolver) Key() suggestionKey {
	return suggestionKey{
		searchContextSpec: s.searchContext.Spec(),
	}
}

type suggestionKey struct {
	repoName          string
	repoRev           string
	file              string
	symbol            string
	lang              string
	url               string
	searchContextSpec string
}

type searchSuggestionsArgs struct {
	First *int32
}

func (a *searchSuggestionsArgs) applyDefaultsAndConstraints() {
	if a.First == nil || *a.First < 0 || *a.First > maxSearchSuggestions {
		n := int32(maxSearchSuggestions)
		a.First = &n
	}
}

type showSearchSuggestionResolvers func() ([]SearchSuggestionResolver, error)

var (
	mockShowRepoSuggestions showSearchSuggestionResolvers
	mockShowFileSuggestions showSearchSuggestionResolvers
	mockShowLangSuggestions showSearchSuggestionResolvers
	mockShowSymbolMatches   showSearchSuggestionResolvers
)

func (r *searchResolver) showRepoSuggestions(ctx context.Context) ([]SearchSuggestionResolver, error) {
	if mockShowRepoSuggestions != nil {
		return mockShowRepoSuggestions()
	}

	// * If query contains only a single term, treat it as a repo field here and ignore the other repo queries.
	// * If only repo fields (except 1 term in query), show repo suggestions.

	hasSingleField := len(r.Query.Fields()) == 1
	hasTwoFields := len(r.Query.Fields()) == 2
	hasSingleContextField := len(r.Query.Values(query.FieldContext)) == 1
	var effectiveRepoFieldValues []string
	if len(r.Query.Values(query.FieldDefault)) == 1 && (hasSingleField || (hasTwoFields && hasSingleContextField)) {
		effectiveRepoFieldValues = append(effectiveRepoFieldValues, r.Query.Values(query.FieldDefault)[0].ToString())
	} else if len(r.Query.Values(query.FieldRepo)) > 0 && hasSingleField {
		effectiveRepoFieldValues, _ = r.Query.Repositories()
	}

	// If we have a query which is not valid, just ignore it since this is for a suggestion.
	i := 0
	for _, v := range effectiveRepoFieldValues {
		if _, err := regexp.Compile(v); err == nil {
			effectiveRepoFieldValues[i] = v
			i++
		}
	}
	effectiveRepoFieldValues = effectiveRepoFieldValues[:i]

	if len(effectiveRepoFieldValues) > 0 || hasSingleContextField {
		repoOptions := r.toRepoOptions(r.Query,
			resolveRepositoriesOpts{
				effectiveRepoFieldValues: effectiveRepoFieldValues,
				limit:                    maxSearchSuggestions,
			})

		// TODO(tsenart): Figure out what to do with this instance of resolveRepositories.
		//  I think we're getting rid of GraphQL suggestions code, so this might be a non-issue.
		resolved, err := r.resolveRepositories(ctx, repoOptions)
		resolvers := make([]SearchSuggestionResolver, 0, len(resolved.RepoRevs))
		for i, rev := range resolved.RepoRevs {
			resolvers = append(resolvers, repositorySuggestionResolver{
				repo: NewRepositoryResolver(r.db, rev.Repo.ToRepo()),
				// Encode the returned order in score.
				score: math.MaxInt32 - i,
			})
		}

		return resolvers, err
	}
	return nil, nil
}

func (r *searchResolver) showFileSuggestions(ctx context.Context) ([]SearchSuggestionResolver, error) {
	if mockShowFileSuggestions != nil {
		return mockShowFileSuggestions()
	}

	// If only repos and files are specified (and at most 1 term), then show file
	// suggestions.  If the query has a single term, then consider it to be a `file:` filter (to
	// make it easy to jump to files by just typing in their name, not `file:<their name>`).
	hasOnlyEmptyRepoField := len(r.Query.Values(query.FieldRepo)) > 0 && allEmptyStrings(r.Query.RegexpPatterns(query.FieldRepo)) && len(r.Query.Fields()) == 1
	hasRepoOrFileFields := len(r.Query.Values(query.FieldRepo)) > 0 || len(r.Query.Values(query.FieldFile)) > 0
	if !hasOnlyEmptyRepoField && hasRepoOrFileFields && len(r.Query.Values(query.FieldDefault)) <= 1 {
		ctx, cancel := context.WithTimeout(ctx, 1*time.Second)
		defer cancel()
		return r.suggestFilePaths(ctx, maxSearchSuggestions)
	}
	return nil, nil
}

func (r *searchResolver) showLangSuggestions(ctx context.Context) ([]SearchSuggestionResolver, error) {
	if mockShowLangSuggestions != nil {
		return mockShowLangSuggestions()
	}

	// The "repo:" field must be specified for showing language suggestions.
	// For performance reasons, only try to get languages of the first repository found
	// within the scope of the "repo:" field value.
	if len(r.Query.Values(query.FieldRepo)) == 0 {
		return nil, nil
	}
	effectiveRepoFieldValues, _ := r.Query.Repositories()

	validValues := effectiveRepoFieldValues[:0]
	for _, v := range effectiveRepoFieldValues {
		if i := strings.LastIndexByte(v, '@'); i > -1 {
			// Strip off the @revision suffix so that we can use
			// the trigram index on the name column in Postgres.
			v = v[:i]
		}

		if _, err := regexp.Compile(v); err == nil {
			validValues = append(validValues, v)
		}
	}
	if len(validValues) == 0 {
		return nil, nil
	}

	// Only care about the first found repository.
	repos, err := backend.Repos.List(ctx, database.ReposListOptions{
		IncludePatterns: validValues,
		LimitOffset: &database.LimitOffset{
			Limit: 1,
		},
	})
	if err != nil || len(repos) == 0 {
		return nil, err
	}
	repo := repos[0]

	ctx, cancel := context.WithTimeout(ctx, 1*time.Second)
	defer cancel()

	commitID, err := backend.Repos.ResolveRev(ctx, repo, "")
	if err != nil {
		return nil, err
	}

	inventory, err := backend.Repos.GetInventory(ctx, repo, commitID, false)
	if err != nil {
		return nil, err
	}

	resolvers := make([]SearchSuggestionResolver, 0, len(inventory.Languages))
	for _, l := range inventory.Languages {
		resolvers = append(resolvers, languageSuggestionResolver{
			lang:  &languageResolver{name: strings.ToLower(l.Name)},
			score: math.MaxInt32,
		})
	}

	return resolvers, err
}

func (r *searchResolver) showSymbolMatches(ctx context.Context) ([]SearchSuggestionResolver, error) {
	if mockShowSymbolMatches != nil {
		return mockShowSymbolMatches()
	}

	b, err := query.ToBasicQuery(r.Query)
	if err != nil {
		return nil, err
	}
	if !query.IsPatternAtom(b) {
		// Not an atomic pattern, can't guarantee it will behave well.
		return nil, nil
	}

	args, jobs, err := r.toSearchInputs(r.Query)
	if err != nil {
		return nil, err
	}
	args.ResultTypes = result.TypeSymbol

	results, err := r.doResults(ctx, args, jobs)
	if errors.Is(err, context.DeadlineExceeded) {
		err = nil
	}
	if err != nil {
		return nil, err
	}
	if results == nil {
		return []SearchSuggestionResolver{}, nil
	}

	suggestions := make([]SearchSuggestionResolver, 0)
	for _, match := range results.Matches {
		fileMatch, ok := match.(*result.FileMatch)
		if !ok {
			continue
		}
		for _, sm := range fileMatch.Symbols {
			score := 20
			if sm.Symbol.Parent == "" {
				score++
			}
			if len(sm.Symbol.Name) < 12 {
				score++
			}
			switch sm.Symbol.LSPKind() {
			case lsp.SKFunction, lsp.SKMethod:
				score += 2
			case lsp.SKClass:
				score += 3
			}
			repoName := strings.ToLower(string(sm.File.Repo.Name))
			fileName := strings.ToLower(sm.File.Path)
			symbolName := strings.ToLower(sm.Symbol.Name)
			if len(sm.Symbol.Name) >= 4 && strings.Contains(repoName+fileName, symbolName) {
				score++
			}
			suggestions = append(suggestions, symbolSuggestionResolver{
				symbol: symbolResolver{
					db: r.db,
					commit: NewGitCommitResolver(
						r.db,
						NewRepositoryResolver(r.db, fileMatch.Repo.ToRepo()),
						fileMatch.CommitID,
						nil,
					),
					SymbolMatch: sm,
				},
				score: score,
			})
		}
	}

	sortSearchSuggestions(suggestions)
	const maxBoostedSymbolResults = 3
	boost := maxBoostedSymbolResults
	if len(suggestions) < boost {
		boost = len(suggestions)
	}
	if boost > 0 {
		for i := 0; i < boost; i++ {
			if res, ok := suggestions[i].(symbolSuggestionResolver); ok {
				res.score += 200
				suggestions[i] = res
			}
		}
	}

	return suggestions, nil
}

// showFilesWithTextMatches returns a suggester bounded by `first`.
func (r *searchResolver) showFilesWithTextMatches(first int32) suggester {
	return func(ctx context.Context) ([]SearchSuggestionResolver, error) {
		// If terms are specified, then show files that have text matches. Set an aggressive timeout
		// to avoid delaying repo and file suggestions for too long.
		ctx, cancel := context.WithTimeout(ctx, 500*time.Millisecond)
		defer cancel()
		if len(r.Query.Values(query.FieldDefault)) > 0 {
			searchArgs, jobs, err := r.toSearchInputs(r.Query)
			if err != nil {
				return nil, err
			}
			searchArgs.ResultTypes = result.TypeFile // only "file" result type
			results, err := r.doResults(ctx, searchArgs, jobs)
			if err == context.DeadlineExceeded {
				err = nil // don't log as error below
			}
			var suggestions []SearchSuggestionResolver
			if results != nil {
				if len(results.Matches) > int(first) {
					results.Matches = results.Matches[:first]
				}
				suggestions = make([]SearchSuggestionResolver, 0, len(results.Matches))
				for i, res := range results.Matches {
					if fm, ok := res.(*result.FileMatch); ok {
						fmResolver := &FileMatchResolver{
							db:           r.db,
							FileMatch:    *fm,
							RepoResolver: NewRepositoryResolver(r.db, fm.Repo.ToRepo()),
						}
						suggestions = append(suggestions, gitTreeSuggestionResolver{
							gitTreeEntry: fmResolver.File(),
							score:        len(results.Matches) - i,
						})
					}
				}
			}
			return suggestions, err
		}
		return nil, nil
	}
}

func (r *searchResolver) showSearchContextSuggestions(ctx context.Context) ([]SearchSuggestionResolver, error) {
	if EnterpriseResolvers.searchContextsResolver == nil {
		return []SearchSuggestionResolver{}, nil
	}

	hasSingleContextField := len(r.Query.Values(query.FieldContext)) == 1
	if !hasSingleContextField {
		return nil, nil
	}
	searchContextSpec, _ := r.Query.StringValue(query.FieldContext)
	parsedSearchContextSpec := searchcontexts.ParseSearchContextSpec(searchContextSpec)
	searchContexts := []*types.SearchContext{}

	autoDefinedSearchContexts, err := searchcontexts.GetAutoDefinedSearchContexts(ctx, r.db)
	if err != nil {
		return nil, err
	}
	for _, searchContext := range autoDefinedSearchContexts {
		matchesName := parsedSearchContextSpec.SearchContextName != "" && strings.Contains(searchContext.Name, parsedSearchContextSpec.SearchContextName)
		matchesNamespace := parsedSearchContextSpec.NamespaceName != "" && (strings.Contains(searchContext.NamespaceUserName, parsedSearchContextSpec.NamespaceName) ||
			strings.Contains(searchContext.NamespaceOrgName, parsedSearchContextSpec.NamespaceName))
		if matchesName || matchesNamespace {
			searchContexts = append(searchContexts, searchContext)
		}
	}

	userDefinedSearchContexts, err := database.SearchContexts(r.db).ListSearchContexts(
		ctx,
		database.ListSearchContextsPageOptions{First: maxSearchSuggestions},
		database.ListSearchContextsOptions{
			Name:              parsedSearchContextSpec.SearchContextName,
			NamespaceName:     parsedSearchContextSpec.NamespaceName,
			OrderBy:           database.SearchContextsOrderBySpec,
			OrderByDescending: true,
		},
	)
	if err != nil {
		return nil, err
	}

	searchContexts = append(searchContexts, userDefinedSearchContexts...)
	searchContextsResolvers := EnterpriseResolvers.searchContextsResolver.SearchContextsToResolvers(searchContexts)
	suggestions := make([]SearchSuggestionResolver, 0, len(searchContextsResolvers))
	for i, searchContextResolver := range searchContextsResolvers {
		suggestions = append(suggestions, &searchContextSuggestionResolver{
			searchContext: searchContextResolver,
			score:         len(searchContextsResolvers) - i,
		})
	}
	return suggestions, nil
}

type suggester func(ctx context.Context) ([]SearchSuggestionResolver, error)

func (r *searchResolver) Suggestions(ctx context.Context, args *searchSuggestionsArgs) ([]SearchSuggestionResolver, error) {
	// If globbing is activated, convert regex patterns of repo, file, and repohasfile
	// from "field:^foo$" to "field:^foo".
	globbing := false
	if getBoolPtr(r.UserSettings.SearchGlobbing, false) {
		globbing = true
	}
	if globbing {
		r.Query = query.FuzzifyRegexPatterns(r.Query)
	}

	args.applyDefaultsAndConstraints()

	if len(r.Query) == 0 {
		return nil, nil
	}

	// Only suggest for type:file.
	typeValues, _ := r.Query.StringValues(query.FieldType)
	for _, resultType := range typeValues {
		if resultType != "file" {
			return nil, nil
		}
	}

	if query.ContainsPredicate(r.Query) {
		// Query contains a predicate that that may first need to be
		// evaluated to provide suggestions (e.g., for repos), or we
		// can't guarantee it will behave well. Evaluating predicates can
		// be expensive, so punt suggestions for queries with them.
		return nil, nil
	}

	if b, err := query.ToBasicQuery(r.Query); err != nil || !query.IsPatternAtom(b) {
		// Query is a search expression that contains 'or' operators,
		// either on filters or patterns. Since it is not a basic query
		// with an atomic pattern, we can't guarantee suggestions behave
		// well--do not return suggestions.
		return nil, nil
	}

	suggesters := []suggester{
		r.showRepoSuggestions,
		r.showFileSuggestions,
		r.showLangSuggestions,
		r.showSymbolMatches,
		r.showFilesWithTextMatches(*args.First),
		r.showSearchContextSuggestions,
	}

	// Run suggesters.
	var (
		allSuggestions []SearchSuggestionResolver
		mu             sync.Mutex
		par            = parallel.NewRun(len(suggesters))
	)
	for _, suggester := range suggesters {
		par.Acquire()
		go func(suggester func(ctx context.Context) ([]SearchSuggestionResolver, error)) {
			defer par.Release()
			ctx, cancel := context.WithTimeout(ctx, 3*time.Second)
			defer cancel()
			suggestions, err := suggester(ctx)
			if err == nil {
				mu.Lock()
				allSuggestions = append(allSuggestions, suggestions...)
				mu.Unlock()
			} else {
				if errors.IsAny(err, context.DeadlineExceeded, context.Canceled) {
					log15.Warn("search suggestions exceeded deadline (skipping)", "query", r.rawQuery())
				} else if !errcode.IsBadRequest(err) {
					// We exclude bad user input. Note that this means that we
					// may have some tokens in the input that are valid, but
					// typing something "bad" results in no suggestions from the
					// this suggester. In future we should just ignore the bad
					// token.
					par.Error(err)
				}
			}
		}(suggester)
	}
	if err := par.Wait(); err != nil {
		if len(allSuggestions) == 0 {
			return nil, err
		}
		// If we got partial results, only log the error and return partial results
		log15.Error("error getting search suggestions: ", "error", err)
	}

	// Eliminate duplicates.
	seen := make(map[suggestionKey]struct{}, len(allSuggestions))
	uniqueSuggestions := allSuggestions[:0]
	for _, s := range allSuggestions {
		k := s.Key()
		if _, dup := seen[k]; !dup {
			uniqueSuggestions = append(uniqueSuggestions, s)
			seen[k] = struct{}{}
		}
	}
	allSuggestions = uniqueSuggestions

	sortSearchSuggestions(allSuggestions)
	if len(allSuggestions) > int(*args.First) {
		allSuggestions = allSuggestions[:*args.First]
	}

	return allSuggestions, nil
}

func allEmptyStrings(ss1, ss2 []string) bool {
	for _, s := range ss1 {
		if s != "" {
			return false
		}
	}
	for _, s := range ss2 {
		if s != "" {
			return false
		}
	}
	return true
}

type languageResolver struct {
	name string
}

func (r *languageResolver) Name() string { return r.name }
