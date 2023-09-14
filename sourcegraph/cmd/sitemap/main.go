package main

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/snabb/sitemap"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/lib/codeintel/lsif/protocol"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func main() {
	gen := &generator{
		graphQLURL:      "https://sourcegraph.com/.api/graphql",
		token:           os.Getenv("SRC_ACCESS_TOKEN"),
		outDir:          "sitemap/",
		queryDatabase:   "sitemap_query.db",
		progressUpdates: 10 * time.Second,
	}
	if err := gen.generate(context.Background()); err != nil {
		gen.logger.Warn("failed to generate", log.Error(err))
		os.Exit(-1)
	}
	gen.logger.Info("generated sitemap", log.String("out", gen.outDir))
}

type generator struct {
	graphQLURL      string
	token           string
	outDir          string
	queryDatabase   string
	progressUpdates time.Duration
	logger          log.Logger

	db        *queryDatabase
	gqlClient *graphQLClient
}

// generate generates the sitemap files to the specified directory.
func (g *generator) generate(ctx context.Context) error {
	if err := os.MkdirAll(g.outDir, 0700); err != nil {
		return errors.Wrap(err, "MkdirAll")
	}
	if err := os.MkdirAll(filepath.Dir(g.queryDatabase), 0700); err != nil {
		return errors.Wrap(err, "MkdirAll")
	}

	// The query database caches our GraphQL queries across multiple runs, as well as allows us to
	// update the sitemap to include new repositories / pages without re-querying everything which
	// would be very expensive. It's a simple on-disk key-vaue store (bbolt).
	var err error
	g.db, err = openQueryDatabase(g.queryDatabase)
	if err != nil {
		return errors.Wrap(err, "openQueryDatabase")
	}
	defer g.db.close()

	g.gqlClient = &graphQLClient{
		URL:   g.graphQLURL,
		Token: g.token,
	}

	// Provide ability to clear specific cache keys (i.e. specific types of GraphQL requests.)
	clearCacheKeys := strings.Fields(os.Getenv("CLEAR_CACHE_KEYS"))
	if len(clearCacheKeys) > 0 {
		for _, key := range clearCacheKeys {
			g.logger.Info("clearing cache key", log.String("key", key))
			if err := g.db.delete(key); err != nil {
				g.logger.Info("failed to clear cache key", log.String("key", key), log.Error(err))
			}
		}
	}
	listCacheKeys, _ := strconv.ParseBool(os.Getenv("LIST_CACHE_KEYS"))
	if listCacheKeys {
		keys, err := g.db.keys()
		if err != nil {
			g.logger.Warn("failed to list cache keys", log.Error(err))
		}
		for _, key := range keys {
			g.logger.Info("listing cache keys", log.String("key", key))
		}
	}

	// Build a set of Go repos that have LSIF indexes.
	indexedGoRepos := map[string][]gqlLSIFIndex{}
	lastUpdate := time.Now()
	queried := 0
	if err := g.eachLsifIndex(ctx, func(each gqlLSIFIndex, total uint64) error {
		if time.Since(lastUpdate) >= g.progressUpdates {
			lastUpdate = time.Now()
			g.logger.Info("progress: discovered LSIF indexes", log.Int("n", queried), log.Uint64("of", total))
		}
		queried++
		if strings.Contains(each.InputIndexer, "lsif-go") {
			repoName := each.ProjectRoot.Repository.Name
			indexedGoRepos[repoName] = append(indexedGoRepos[repoName], each)
		}
		return nil
	}); err != nil {
		return err
	}

	// Fetch documentation path info for each chosen repo with LSIF indexes.
	var (
		pagesByRepo    = map[string][]string{}
		totalPages     = 0
		totalStars     uint64
		missingAPIDocs = 0
	)
	lastUpdate = time.Now()
	queried = 0
	for repoName, indexes := range indexedGoRepos {
		if time.Since(lastUpdate) >= g.progressUpdates {
			lastUpdate = time.Now()
			g.logger.Info("progress: discovered API docs pages for repo", log.Int("n", queried), log.Int("of", len(indexedGoRepos)))
		}
		totalStars += indexes[0].ProjectRoot.Repository.Stars
		pathInfo, err := g.fetchDocPathInfo(ctx, gqlDocPathInfoVars{RepoName: repoName})
		queried++
		if pathInfo == nil || (err != nil && strings.Contains(err.Error(), "page not found")) {
			if err != nil {
				missingAPIDocs++
			}
			continue
		}
		if err != nil {
			return errors.Wrap(err, "fetchDocPathInfo")
		}
		var walk func(node DocumentationPathInfoResult)
		walk = func(node DocumentationPathInfoResult) {
			pagesByRepo[repoName] = append(pagesByRepo[repoName], node.PathID)
			for _, child := range node.Children {
				walk(child)
			}
		}
		walk(*pathInfo)
		totalPages += len(pagesByRepo[repoName])
	}

	// Fetch all documentation pages.
	queried = 0
	unexpectedMissingPages := 0
	var docsSubPagesByRepo [][2]string
	for repoName, pagePathIDs := range pagesByRepo {
		for _, pathID := range pagePathIDs {
			page, err := g.fetchDocPage(ctx, gqlDocPageVars{RepoName: repoName, PathID: pathID})
			if page == nil || (err != nil && strings.Contains(err.Error(), "page not found")) {
				g.logger.Warn("unexpected: API docs page missing after reportedly existing", log.String("repo", repoName), log.String("pathID", pathID), log.Error(err))
				unexpectedMissingPages++
				continue
			}
			if err != nil {
				return err
			}
			queried++
			if time.Since(lastUpdate) >= g.progressUpdates {
				lastUpdate = time.Now()
				g.logger.Info("progress: got API docs page", log.Int("n", queried), log.Int("of", totalPages))
			}

			var walk func(node *DocumentationNode)
			walk = func(node *DocumentationNode) {
				goodDetail := len(node.Detail.String()) > 100
				goodTags := !nodeIsExcluded(node, protocol.TagPrivate)
				if goodDetail && goodTags {
					docsSubPagesByRepo = append(docsSubPagesByRepo, [2]string{repoName, node.PathID})
				}

				for _, child := range node.Children {
					if child.Node != nil {
						walk(child.Node)
					}
				}
			}
			walk(page)
		}
	}

	var (
		mu                                     sync.Mutex
		docsSubPages                           []string
		workers                                = 300
		index                                  = 0
		subPagesWithZeroReferences             = 0
		subPagesWithOneOrMoreExternalReference = 0
	)
	queried = 0
	for i := 0; i < workers; i++ {
		go func() {
			for {
				mu.Lock()
				if index >= len(docsSubPagesByRepo) {
					mu.Unlock()
					return
				}
				pair := docsSubPagesByRepo[index]
				repoName, pathID := pair[0], pair[1]
				index++

				if time.Since(lastUpdate) >= g.progressUpdates {
					lastUpdate = time.Now()
					g.logger.Info("progress: got API docs usage examples", log.Int("n", index), log.Int("of", len(docsSubPagesByRepo)))
				}
				mu.Unlock()

				references, err := g.fetchDocReferences(ctx, gqlDocReferencesVars{
					RepoName: repoName,
					PathID:   pathID,
					First:    intPtr(3),
				})
				if err != nil {
					g.logger.Warn("unexpected: error getting references", log.String("repo", repoName), log.String("pathID", pathID), log.Error(err))
				} else {
					refs := references.Data.Repository.Commit.Tree.LSIF.DocumentationReferences.Nodes
					if len(refs) >= 1 {
						externalReferences := 0
						for _, ref := range refs {
							if ref.Resource.Repository.Name != repoName {
								externalReferences++
							}
						}
						// TODO(apidocs): it would be great if more repos had external usage examples. In practice though, less than 2%
						// do today. This is because we haven't indexed many repos yet.
						if externalReferences > 0 {
							subPagesWithOneOrMoreExternalReference++
						}
						mu.Lock()
						docsPath := pathID
						if strings.Contains(docsPath, "#") {
							split := strings.Split(docsPath, "#")
							if split[0] == "/" {
								docsPath = "?" + split[1]
							} else {
								docsPath = split[0] + "?" + split[1]
							}
						}
						docsSubPages = append(docsSubPages, repoName+"/-/docs"+docsPath)
						mu.Unlock()
					} else {
						subPagesWithZeroReferences++
					}
				}
			}
		}()
	}
	for {
		time.Sleep(1 * time.Second)
		mu.Lock()
		if index >= len(docsSubPagesByRepo) {
			mu.Unlock()
			break
		}
		mu.Unlock()
	}

	g.logger.Info("found Go API docs pages", log.Int("count", totalPages))
	g.logger.Info("found Go API docs sub-pages", log.Int("count", len(docsSubPages)))
	g.logger.Info("Go API docs sub-pages with 1+ external reference", log.Int("count", subPagesWithOneOrMoreExternalReference))
	g.logger.Info("Go API docs sub-pages with 0 references", log.Int("count", subPagesWithZeroReferences))
	g.logger.Info("spanning", log.Int("repositories", len(indexedGoRepos)), log.Uint64("stars", totalStars))
	g.logger.Info("Go repos missing API docs", log.Int("count", missingAPIDocs))

	sort.Strings(docsSubPages)
	var (
		sitemapIndex = sitemap.NewSitemapIndex()
		addedURLs    = 0
		sitemaps     []*sitemap.Sitemap
		addSitemap   = func() *sitemap.Sitemap {
			var sm = sitemap.New()
			url := &sitemap.URL{Loc: fmt.Sprintf("https://sourcegraph.com/sitemap_%03d.xml.gz", len(sitemaps))}
			sitemapIndex.Add(url)
			sitemaps = append(sitemaps, sm)
			return sm
		}
		sm *sitemap.Sitemap = addSitemap()
	)
	for _, docSubPage := range docsSubPages {
		if addedURLs >= 50000 {
			addedURLs = 0
			sm = addSitemap()
		}
		addedURLs++
		sm.Add(&sitemap.URL{
			Loc:        "https://sourcegraph.com/" + docSubPage,
			ChangeFreq: sitemap.Weekly,
		})
	}

	{
		outFile, err := os.Create(filepath.Join(g.outDir, "sitemap.xml.gz"))
		if err != nil {
			return errors.Wrap(err, "failed to create sitemap.xml.gz file")
		}
		defer outFile.Close()
		writer := gzip.NewWriter(outFile)
		defer writer.Close()
		_, err = sitemapIndex.WriteTo(writer)
		if err != nil {
			return errors.Wrap(err, "failed to write sitemap.xml.gz")
		}
	}
	for index, sm := range sitemaps {
		fileName := fmt.Sprintf("sitemap_%03d.xml.gz", index)
		outFile, err := os.Create(filepath.Join(g.outDir, fileName))
		if err != nil {
			return errors.Wrap(err, fmt.Sprintf("failed to create %s file", fileName))
		}
		defer outFile.Close()
		writer := gzip.NewWriter(outFile)
		defer writer.Close()
		_, err = sm.WriteTo(writer)
		if err != nil {
			return errors.Wrap(err, fmt.Sprintf("failed to write %s", fileName))
		}
	}

	g.logger.Info("you may now upload the generated sitemap/")

	return nil
}

func (g *generator) eachLsifIndex(ctx context.Context, each func(index gqlLSIFIndex, total uint64) error) error {
	var (
		hasNextPage = true
		cursor      *string
	)
	for hasNextPage {
		retries := 0
	retry:
		lsifIndexes, err := g.fetchLsifIndexes(ctx, gqlLSIFIndexesVars{
			State: strPtr("COMPLETED"),
			First: intPtr(5000),
			After: cursor,
		})
		if err != nil {
			retries++
			if maxRetries := 10; retries < maxRetries {
				g.logger.Warn("error listing LSIF indexes", log.Int("retry", retries), log.Int("of", maxRetries))
				goto retry
			}
			return err
		}

		for _, index := range lsifIndexes.Data.LsifIndexes.Nodes {
			if err := each(index, lsifIndexes.Data.LsifIndexes.TotalCount); err != nil {
				return err
			}
		}
		hasNextPage = lsifIndexes.Data.LsifIndexes.PageInfo.HasNextPage
		cursor = lsifIndexes.Data.LsifIndexes.PageInfo.EndCursor
	}
	return nil
}

func (g *generator) fetchLsifIndexes(ctx context.Context, vars gqlLSIFIndexesVars) (*gqlLSIFIndexesResponse, error) {
	data, err := g.db.request(requestKey{RequestName: "LsifIndexes", Vars: vars}, func() ([]byte, error) {
		return g.gqlClient.requestGraphQL(ctx, "SitemapLsifIndexes", gqlLSIFIndexesQuery, vars)
	})
	if err != nil {
		return nil, err
	}
	var resp gqlLSIFIndexesResponse
	return &resp, json.Unmarshal(data, &resp)
}

func (g *generator) fetchDocPathInfo(ctx context.Context, vars gqlDocPathInfoVars) (*DocumentationPathInfoResult, error) {
	data, err := g.db.request(requestKey{RequestName: "DocPathInfo", Vars: vars}, func() ([]byte, error) {
		return g.gqlClient.requestGraphQL(ctx, "SitemapDocPathInfo", gqlDocPathInfoQuery, vars)
	})
	if err != nil {
		return nil, err
	}
	var resp gqlDocPathInfoResponse
	if err := json.Unmarshal(data, &resp); err != nil {
		return nil, errors.Wrap(err, "Unmarshal GraphQL response")
	}
	payload := resp.Data.Repository.Commit.Tree.LSIF.DocumentationPathInfo
	if payload == "" {
		return nil, nil
	}
	var result DocumentationPathInfoResult
	if err := json.Unmarshal([]byte(payload), &result); err != nil {
		return nil, errors.Wrap(err, "Unmarshal DocumentationPathInfoResult")
	}
	return &result, nil
}

func (g *generator) fetchDocPage(ctx context.Context, vars gqlDocPageVars) (*DocumentationNode, error) {
	data, err := g.db.request(requestKey{RequestName: "DocPage", Vars: vars}, func() ([]byte, error) {
		return g.gqlClient.requestGraphQL(ctx, "SitemapDocPage", gqlDocPageQuery, vars)
	})
	if err != nil {
		return nil, err
	}
	var resp gqlDocPageResponse
	if err := json.Unmarshal(data, &resp); err != nil {
		return nil, errors.Wrap(err, "Unmarshal GraphQL response")
	}
	payload := resp.Data.Repository.Commit.Tree.LSIF.DocumentationPage.Tree
	if payload == "" {
		return nil, nil
	}
	var result DocumentationNode
	if err := json.Unmarshal([]byte(payload), &result); err != nil {
		return nil, errors.Wrap(err, "Unmarshal DocumentationNode")
	}
	return &result, nil
}

func (g *generator) fetchDocReferences(ctx context.Context, vars gqlDocReferencesVars) (*gqlDocReferencesResponse, error) {
	data, err := g.db.request(requestKey{RequestName: "DocReferences", Vars: vars}, func() ([]byte, error) {
		return g.gqlClient.requestGraphQL(ctx, "SitemapDocReferences", gqlDocReferencesQuery, vars)
	})
	if err != nil {
		return nil, err
	}
	var resp gqlDocReferencesResponse
	return &resp, json.Unmarshal(data, &resp)
}

func nodeIsExcluded(node *DocumentationNode, excludingTags ...protocol.Tag) bool {
	for _, tag := range node.Documentation.Tags {
		for _, excludedTag := range excludingTags {
			if tag == excludedTag {
				return true
			}
		}
	}
	return false
}
