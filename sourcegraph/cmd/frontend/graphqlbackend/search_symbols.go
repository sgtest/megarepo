package graphqlbackend

import (
	"context"
	"fmt"
	"log"
	"net/url"
	"strings"
	"sync"

	"github.com/neelance/parallel"
	opentracing "github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/search"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/search/query"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	"github.com/sourcegraph/sourcegraph/pkg/lsp"
	"github.com/sourcegraph/sourcegraph/pkg/symbols/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/trace"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
	"github.com/sourcegraph/sourcegraph/xlang/uri"
)

var mockSearchSymbols func(ctx context.Context, args *search.Args, limit int) (res []*fileMatchResolver, common *searchResultsCommon, err error)

// searchSymbols searches the given repos in parallel for symbols matching the given search query
// it can be used for both search suggestions and search results
//
// May return partial results and an error
func searchSymbols(ctx context.Context, args *search.Args, limit int) (res []*fileMatchResolver, common *searchResultsCommon, err error) {
	if mockSearchSymbols != nil {
		return mockSearchSymbols(ctx, args, limit)
	}

	tr, ctx := trace.New(ctx, "Search symbols", fmt.Sprintf("query: %+v, numRepoRevs: %d", args.Pattern, len(args.Repos)))
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	if args.Pattern.Pattern == "" {
		return nil, nil, nil
	}

	ctx, cancelAll := context.WithCancel(ctx)
	defer cancelAll()

	common = &searchResultsCommon{}
	var (
		run = parallel.NewRun(20)
		mu  sync.Mutex
	)
	for _, repoRevs := range args.Repos {
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
			repoSymbols, repoErr := searchSymbolsInRepo(ctx, repoRevs, args.Pattern, args.Query, limit)
			if repoErr != nil {
				tr.LogFields(otlog.String("repo", string(repoRevs.Repo.URI)), otlog.String("repoErr", repoErr.Error()), otlog.Bool("timeout", errcode.IsTimeout(repoErr)), otlog.Bool("temporary", errcode.IsTemporary(repoErr)))
			}
			mu.Lock()
			defer mu.Unlock()
			limitHit := len(res) > limit
			repoErr = handleRepoSearchResult(common, *repoRevs, limitHit, false, repoErr)
			if repoErr != nil {
				if ctx.Err() == nil || errors.Cause(repoErr) != ctx.Err() {
					// Only record error if it's not directly caused by a context error.
					run.Error(repoErr)
				}
			} else {
				common.searched = append(common.searched, repoRevs.Repo)
			}
			if repoSymbols != nil {
				res = append(res, repoSymbols...)
				if limitHit {
					cancelAll()
				}
			}
		})
	}
	err = run.Wait()

	if len(res) > limit {
		common.limitHit = true
		res = res[:limit]
	}
	return res, common, err
}

func searchSymbolsInRepo(ctx context.Context, repoRevs *search.RepositoryRevisions, patternInfo *search.PatternInfo, query *query.Query, limit int) (res []*fileMatchResolver, err error) {
	span, ctx := opentracing.StartSpanFromContext(ctx, "Search symbols in repo")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.LogFields(otlog.Error(err))
		}
		span.Finish()
	}()
	span.SetTag("repo", string(repoRevs.Repo.URI))

	inputRev := repoRevs.RevSpecs()[0]
	span.SetTag("rev", inputRev)
	// Do not trigger a repo-updater lookup (e.g.,
	// backend.{GitRepo,Repos.ResolveRev}) because that would slow this operation
	// down by a lot (if we're looping over many repos). This means that it'll fail if a
	// repo is not on gitserver.
	commitID, err := git.ResolveRevision(ctx, repoRevs.GitserverRepo, nil, inputRev, nil)
	if err != nil {
		return nil, err
	}
	span.SetTag("commit", string(commitID))
	baseURI, err := uri.Parse("git://" + string(repoRevs.Repo.URI) + "?" + url.QueryEscape(inputRev))
	if err != nil {
		return nil, err
	}

	symbols, err := backend.Symbols.ListTags(ctx, protocol.SearchArgs{
		Repo:            repoRevs.Repo.URI,
		CommitID:        commitID,
		Query:           patternInfo.Pattern,
		IsCaseSensitive: patternInfo.IsCaseSensitive,
		IsRegExp:        patternInfo.IsRegExp,
		IncludePatterns: patternInfo.IncludePatterns,
		ExcludePattern:  patternInfo.ExcludePattern,
		First:           limit,
	})
	fileMatchesByURI := make(map[string]*fileMatchResolver)
	fileMatches := make([]*fileMatchResolver, 0)
	for _, symbol := range symbols {
		commit := &gitCommitResolver{
			repo:     &repositoryResolver{repo: repoRevs.Repo},
			oid:      gitObjectID(commitID),
			inputRev: &inputRev,
			// NOTE: Not all fields are set, for performance.
		}
		if inputRev != "" {
			commit.inputRev = &inputRev
		}
		symbolRes := toSymbolResolver(symbolToLSPSymbolInformation(symbol, baseURI), strings.ToLower(symbol.Language), commit)
		uri := makeFileMatchURIFromSymbol(symbolRes, inputRev)
		if fileMatch, ok := fileMatchesByURI[uri]; ok {
			fileMatch.symbols = append(fileMatch.symbols, symbolRes)
		} else {
			fileMatch := &fileMatchResolver{
				symbols:  []*symbolResolver{symbolRes},
				uri:      uri,
				repo:     symbolRes.location.resource.commit.repo.repo,
				commitID: api.CommitID(symbolRes.location.resource.commit.oid),
			}
			fileMatchesByURI[uri] = fileMatch
			fileMatches = append(fileMatches, fileMatch)
		}
	}
	return fileMatches, err
}

// makeFileMatchURIFromSymbol makes a git://repo?rev#path URI from a symbolResolver to use in a fileMatchResolver
func makeFileMatchURIFromSymbol(symbolResolver *symbolResolver, inputRev string) string {
	uri := "git:/" + string(symbolResolver.location.resource.commit.repo.URL())
	if inputRev != "" {
		uri += "?" + inputRev
	}
	uri += "#" + symbolResolver.location.resource.path
	return uri
}

// symbolToLSPSymbolInformation converts a symbols service Symbol struct to an LSP SymbolInformation
// baseURI is the git://repo?rev base URI for the symbol that is extended with the file path
func symbolToLSPSymbolInformation(s protocol.Symbol, baseURI *uri.URI) lsp.SymbolInformation {
	ch := ctagsSymbolCharacter(s)
	return lsp.SymbolInformation{
		Name:          s.Name + s.Signature,
		ContainerName: s.Parent,
		Kind:          ctagsKindToLSPSymbolKind(s.Kind),
		Location: lsp.Location{
			URI: lsp.DocumentURI(baseURI.WithFilePath(s.Path).String()),
			Range: lsp.Range{
				Start: lsp.Position{Line: s.Line - 1, Character: ch},
				End:   lsp.Position{Line: s.Line - 1, Character: ch + len(s.Name)},
			},
		},
	}
}

// ctagsSymbolCharacter only outputs the line number, not the character (or range). Use the regexp it provides to
// guess the character.
func ctagsSymbolCharacter(s protocol.Symbol) int {
	if s.Pattern == "" {
		return 0
	}
	pattern := strings.TrimPrefix(s.Pattern, "/^")
	i := strings.Index(pattern, s.Name)
	if i >= 0 {
		return i
	}
	return 0
}

func ctagsKindToLSPSymbolKind(kind string) lsp.SymbolKind {
	// Ctags kinds are determined by the parser and do not (in general) match LSP symbol kinds.
	switch kind {
	case "file":
		return lsp.SKFile
	case "module":
		return lsp.SKModule
	case "namespace":
		return lsp.SKNamespace
	case "package", "subprogspec":
		return lsp.SKPackage
	case "class", "type", "service", "typedef", "union", "section", "subtype", "component":
		return lsp.SKClass
	case "method":
		return lsp.SKMethod
	case "property":
		return lsp.SKProperty
	case "field", "member", "anonMember":
		return lsp.SKField
	case "constructor":
		return lsp.SKConstructor
	case "enum", "enumerator":
		return lsp.SKEnum
	case "interface":
		return lsp.SKInterface
	case "function", "func", "subroutine", "macro", "subprogram", "procedure", "command", "singletonMethod":
		return lsp.SKFunction
	case "variable", "var", "functionVar", "define", "alias":
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
	case "enum member", "enumConstant":
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
	log.Printf("Unknown ctags kind: %q", kind)
	return 0
}
