package graphqlbackend

import (
	"context"
	"fmt"
	"sort"
	"strings"

	"github.com/inconshreveable/log15"
	"github.com/neelance/parallel"
	"github.com/opentracing/opentracing-go/ext"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/go-lsp"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

var mockSearchSymbols func(ctx context.Context, args *search.TextParameters, limit int) (res []*FileMatchResolver, stats *streaming.Stats, err error)

// searchSymbols searches the given repos in parallel for symbols matching the given search query
// it can be used for both search suggestions and search results
//
// May return partial results and an error
func searchSymbols(ctx context.Context, db dbutil.DB, args *search.TextParameters, limit int, stream Sender) (err error) {
	if mockSearchSymbols != nil {
		results, stats, err := mockSearchSymbols(ctx, args, limit)
		stream.Send(SearchEvent{
			Results: fileMatchResolversToSearchResults(results),
			Stats:   statsDeref(stats),
		})
		return err
	}

	repos, err := getRepos(ctx, args.RepoPromise)
	if err != nil {
		return err
	}

	tr, ctx := trace.New(ctx, "Search symbols", fmt.Sprintf("query: %+v, numRepoRevs: %d", args.PatternInfo, len(repos)))
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	if args.PatternInfo.Pattern == "" {
		return nil
	}

	ctx, stream, cancel := WithLimit(ctx, stream, limit)
	defer cancel()

	indexed, err := newIndexedSearchRequest(ctx, db, args, symbolRequest, stream)
	if err != nil {
		return err
	}

	run := parallel.NewRun(conf.SearchSymbolsParallelism())

	run.Acquire()
	goroutine.Go(func() {
		defer run.Release()

		err := indexed.Search(ctx, stream)
		if err != nil {
			tr.LogFields(otlog.Error(err))
			// Only record error if we haven't timed out.
			if ctx.Err() == nil {
				cancel()
				run.Error(err)
			}
		}
	})

	for _, repoRevs := range indexed.Unindexed {
		repoRevs := repoRevs
		if ctx.Err() != nil {
			break
		}
		if len(repoRevs.RevSpecs()) == 0 {
			continue
		}
		run.Acquire()
		goroutine.Go(func() {
			defer run.Release()

			matches, err := searchSymbolsInRepo(ctx, repoRevs, args.PatternInfo, limit)
			stats, err := handleRepoSearchResult(repoRevs, len(matches) > limit, false, err)
			stream.Send(SearchEvent{
				Results: fileMatchesToSearchResults(db, matches),
				Stats:   stats,
			})
			if err != nil {
				tr.LogFields(otlog.String("repo", string(repoRevs.Repo.Name)), otlog.Error(err))
				// Only record error if we haven't timed out.
				if ctx.Err() == nil {
					cancel()
					run.Error(err)
				}
			}
		})
	}

	return run.Wait()
}

// limitSymbolResults returns a new version of res containing no more than limit symbol matches.
func limitSymbolResults(res []*FileMatchResolver, limit int) []*FileMatchResolver {
	res2 := make([]*FileMatchResolver, 0, len(res))
	nsym := 0
	for _, r := range res {
		symbols := r.FileMatch.Symbols
		if nsym+len(symbols) > limit {
			symbols = symbols[:limit-nsym]
		}
		if len(symbols) > 0 {
			r2 := *r
			r2.FileMatch.Symbols = symbols
			res2 = append(res2, &r2)
		}
		nsym += len(symbols)
		if nsym >= limit {
			return res2
		}
	}
	return res2
}

// symbolCount returns the total number of symbols in a slice of fileMatchResolvers.
func symbolCount(fmrs []*FileMatchResolver) int {
	nsym := 0
	for _, fmr := range fmrs {
		nsym += len(fmr.FileMatch.Symbols)
	}
	return nsym
}

func searchSymbolsInRepo(ctx context.Context, repoRevs *search.RepositoryRevisions, patternInfo *search.TextPatternInfo, limit int) (res []result.FileMatch, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "Search symbols in repo")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.LogFields(otlog.Error(err))
		}
		span.Finish()
	}()
	span.SetTag("repo", string(repoRevs.Repo.Name))

	inputRev := repoRevs.RevSpecs()[0]
	span.SetTag("rev", inputRev)
	// Do not trigger a repo-updater lookup (e.g.,
	// backend.{GitRepo,Repos.ResolveRev}) because that would slow this operation
	// down by a lot (if we're looping over many repos). This means that it'll fail if a
	// repo is not on gitserver.
	commitID, err := git.ResolveRevision(ctx, repoRevs.GitserverRepo(), inputRev, git.ResolveRevisionOptions{})
	if err != nil {
		return nil, err
	}
	span.SetTag("commit", string(commitID))

	symbols, err := backend.Symbols.ListTags(ctx, search.SymbolsParameters{
		Repo:            repoRevs.Repo.Name,
		CommitID:        commitID,
		Query:           patternInfo.Pattern,
		IsCaseSensitive: patternInfo.IsCaseSensitive,
		IsRegExp:        patternInfo.IsRegExp,
		IncludePatterns: patternInfo.IncludePatterns,
		ExcludePattern:  patternInfo.ExcludePattern,
		// Ask for limit + 1 so we can detect whether there are more results than the limit.
		First: limit + 1,
	})

	// All symbols are from the same repo, so we can just partition them by path
	// to build fileMatches
	symbolsByPath := make(map[string][]*result.Symbol)
	for _, symbol := range symbols {
		cur := symbolsByPath[symbol.Path]
		symbolsByPath[symbol.Path] = append(cur, &symbol)
	}

	// Create file matches from partitioned symbols
	fileMatches := make([]result.FileMatch, 0, len(symbolsByPath))
	for path, symbols := range symbolsByPath {
		file := result.File{
			Path:     path,
			Repo:     repoRevs.Repo,
			CommitID: commitID,
			InputRev: &inputRev,
		}

		symbolMatches := make([]*result.SymbolMatch, 0, len(symbols))
		for _, symbol := range symbols {
			symbolMatches = append(symbolMatches, &result.SymbolMatch{
				File:   &file,
				Symbol: *symbol,
			})
		}

		fileMatches = append(fileMatches, result.FileMatch{
			Symbols: symbolMatches,
			File:    file,
		})
	}

	// Make the results deterministic
	sort.Slice(fileMatches, func(i, j int) bool {
		return fileMatches[i].Path < fileMatches[j].Path
	})
	return fileMatches, err
}

func ctagsKindToLSPSymbolKind(kind string) lsp.SymbolKind {
	// Ctags kinds are determined by the parser and do not (in general) match LSP symbol kinds.
	switch strings.ToLower(kind) {
	case "file":
		return lsp.SKFile
	case "module":
		return lsp.SKModule
	case "namespace":
		return lsp.SKNamespace
	case "package", "packagename", "subprogspec":
		return lsp.SKPackage
	case "class", "type", "service", "typedef", "union", "section", "subtype", "component":
		return lsp.SKClass
	case "method", "methodspec":
		return lsp.SKMethod
	case "property":
		return lsp.SKProperty
	case "field", "member", "anonmember", "recordfield":
		return lsp.SKField
	case "constructor":
		return lsp.SKConstructor
	case "enum", "enumerator":
		return lsp.SKEnum
	case "interface":
		return lsp.SKInterface
	case "function", "func", "subroutine", "macro", "subprogram", "procedure", "command", "singletonmethod":
		return lsp.SKFunction
	case "variable", "var", "functionvar", "define", "alias", "val":
		return lsp.SKVariable
	case "constant", "const":
		return lsp.SKConstant
	case "string", "message", "heredoc":
		return lsp.SKString
	case "number":
		return lsp.SKNumber
	case "bool", "boolean":
		return lsp.SKBoolean
	case "array":
		return lsp.SKArray
	case "object", "literal", "map":
		return lsp.SKObject
	case "key", "label", "target", "selector", "id", "tag":
		return lsp.SKKey
	case "null":
		return lsp.SKNull
	case "enum member", "enumconstant":
		return lsp.SKEnumMember
	case "struct":
		return lsp.SKStruct
	case "event":
		return lsp.SKEvent
	case "operator":
		return lsp.SKOperator
	case "type parameter", "annotation":
		return lsp.SKTypeParameter
	}
	log15.Debug("Unknown ctags kind", "kind", kind)
	return 0
}
