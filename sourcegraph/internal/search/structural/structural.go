package structural

import (
	"context"

	"github.com/opentracing/opentracing-go/log"
	"golang.org/x/sync/errgroup"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/limits"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	searchrepos "github.com/sourcegraph/sourcegraph/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/searcher"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	zoektutil "github.com/sourcegraph/sourcegraph/internal/search/zoekt"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// repoData represents an object of repository revisions to search.
type repoData interface {
	AsList() []*search.RepositoryRevisions
	IsIndexed() bool
}

type IndexedMap map[api.RepoID]*search.RepositoryRevisions

func (m IndexedMap) AsList() []*search.RepositoryRevisions {
	reposList := make([]*search.RepositoryRevisions, 0, len(m))
	for _, repo := range m {
		reposList = append(reposList, repo)
	}
	return reposList
}

func (IndexedMap) IsIndexed() bool {
	return true
}

type UnindexedList []*search.RepositoryRevisions

func (ul UnindexedList) AsList() []*search.RepositoryRevisions {
	return ul
}

func (UnindexedList) IsIndexed() bool {
	return false
}

// searchRepos represent the arguments to a search called over repositories.
type searchRepos struct {
	args    *search.SearcherParameters
	clients job.RuntimeClients
	repoSet repoData
	stream  streaming.Sender
}

// getJob returns a function parameterized by ctx to search over repos.
func (s *searchRepos) getJob(ctx context.Context) func() error {
	return func() error {
		searcherJob := &searcher.TextSearchJob{
			PatternInfo:     s.args.PatternInfo,
			Repos:           s.repoSet.AsList(),
			Indexed:         s.repoSet.IsIndexed(),
			UseFullDeadline: s.args.UseFullDeadline,
			Features:        s.args.Features,
		}

		_, err := searcherJob.Run(ctx, s.clients, s.stream)
		return err
	}
}

func runJobs(ctx context.Context, jobs []*searchRepos) error {
	g, ctx := errgroup.WithContext(ctx)
	for _, j := range jobs {
		g.Go(j.getJob(ctx))
	}
	return g.Wait()
}

// streamStructuralSearch runs structural search jobs and streams the results.
func streamStructuralSearch(ctx context.Context, clients job.RuntimeClients, args *search.SearcherParameters, repos []repoData, stream streaming.Sender) (err error) {
	jobs := []*searchRepos{}
	for _, repoSet := range repos {
		searcherArgs := &search.SearcherParameters{
			PatternInfo:     args.PatternInfo,
			UseFullDeadline: args.UseFullDeadline,
			Features:        args.Features,
		}

		jobs = append(jobs, &searchRepos{clients: clients, args: searcherArgs, stream: stream, repoSet: repoSet})
	}
	return runJobs(ctx, jobs)
}

// retryStructuralSearch runs a structural search with a higher limit file match
// limit so that Zoekt resolves more potential file matches.
func retryStructuralSearch(ctx context.Context, clients job.RuntimeClients, args *search.SearcherParameters, repos []repoData, stream streaming.Sender) error {
	patternCopy := *(args.PatternInfo)
	patternCopy.FileMatchLimit = 1000
	argsCopy := *args
	argsCopy.PatternInfo = &patternCopy
	args = &argsCopy
	return streamStructuralSearch(ctx, clients, args, repos, stream)
}

func runStructuralSearch(ctx context.Context, clients job.RuntimeClients, args *search.SearcherParameters, repos []repoData, stream streaming.Sender) error {
	if args.PatternInfo.FileMatchLimit != limits.DefaultMaxSearchResults {
		// streamStructuralSearch performs a streaming search when the user sets a value
		// for `count`. The first return parameter indicates whether the request was
		// serviced with streaming.
		return streamStructuralSearch(ctx, clients, args, repos, stream)
	}

	// For structural search with default limits we retry if we get no results.
	agg := streaming.NewAggregatingStream()
	err := streamStructuralSearch(ctx, clients, args, repos, agg)

	event := agg.SearchEvent
	if len(event.Results) == 0 && err == nil {
		// retry structural search with a higher limit.
		agg := streaming.NewAggregatingStream()
		err := retryStructuralSearch(ctx, clients, args, repos, agg)
		if err != nil {
			return err
		}

		event = agg.SearchEvent
		if len(event.Results) == 0 {
			// Still no results? Give up.
			clients.Logger.Warn("Structural search gives up after more exhaustive attempt. Results may have been missed.")
			event.Stats.IsLimitHit = false // Ensure we don't display "Show more".
		}
	}

	matches := make([]result.Match, 0, len(event.Results))
	for _, fm := range event.Results {
		if _, ok := fm.(*result.FileMatch); !ok {
			return errors.Errorf("StructuralSearchJob failed to convert results")
		}
		matches = append(matches, fm)
	}

	stream.Send(streaming.SearchEvent{
		Results: matches,
		Stats:   event.Stats,
	})
	return err
}

type SearchJob struct {
	ZoektArgs        *search.ZoektParameters
	SearcherArgs     *search.SearcherParameters
	UseIndex         query.YesNoOnly
	ContainsRefGlobs bool

	RepoOpts search.RepoOptions
}

func (s *SearchJob) Run(ctx context.Context, clients job.RuntimeClients, stream streaming.Sender) (alert *search.Alert, err error) {
	_, ctx, stream, finish := job.StartSpan(ctx, stream, s)
	defer func() { finish(alert, err) }()

	repos := searchrepos.NewResolver(clients.DB)
	return nil, repos.Paginate(ctx, s.RepoOpts, func(page *searchrepos.Resolved) error {
		indexed, unindexed, err := zoektutil.PartitionRepos(
			ctx,
			clients.Logger,
			page.RepoRevs,
			clients.Zoekt,
			search.TextRequest,
			s.UseIndex,
			s.ContainsRefGlobs,
		)
		if err != nil {
			return err
		}

		repoSet := []repoData{UnindexedList(unindexed)}
		if indexed != nil {
			repoSet = append(repoSet, IndexedMap(indexed.RepoRevs))
		}
		return runStructuralSearch(ctx, clients, s.SearcherArgs, repoSet, stream)
	})
}

func (*SearchJob) Name() string {
	return "StructuralSearchJob"
}

func (s *SearchJob) Fields(v job.Verbosity) (res []log.Field) {
	switch v {
	case job.VerbosityMax:
		res = append(res,
			log.Bool("useFullDeadline", s.SearcherArgs.UseFullDeadline),
			log.Bool("containsRefGlobs", s.ContainsRefGlobs),
			log.String("useIndex", string(s.UseIndex)),
			trace.Printf("select", "%q", s.ZoektArgs.Select),
			log.Int32("fileMatchLimit", s.ZoektArgs.FileMatchLimit),
		)
		fallthrough
	case job.VerbosityBasic:
		res = append(res,
			trace.Stringer("query", s.ZoektArgs.Query),
			log.String("type", string(s.ZoektArgs.Typ)),
			trace.Scoped("patternInfo", s.SearcherArgs.PatternInfo.Fields()...),
			trace.Scoped("repoOpts", s.RepoOpts.Tags()...),
		)
	}
	return res
}

func (s *SearchJob) Children() []job.Describer       { return nil }
func (s *SearchJob) MapChildren(job.MapFunc) job.Job { return s }
