package client

import (
	"context"
	"fmt"

	"github.com/grafana/regexp"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/zoekt"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/endpoint"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/job/jobutil"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/searchcontexts"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

type SearchClient interface {
	Plan(
		ctx context.Context,
		version string,
		patternType *string,
		searchQuery string,
		searchMode search.Mode,
		protocol search.Protocol,
		settings *schema.Settings,
		sourcegraphDotComMode bool,
	) (*search.Inputs, error)

	Execute(
		ctx context.Context,
		stream streaming.Sender,
		inputs *search.Inputs,
	) (_ *search.Alert, err error)

	JobClients() job.RuntimeClients
}

func NewSearchClient(logger log.Logger, db database.DB, zoektStreamer zoekt.Streamer, searcherURLs *endpoint.Map, searcherGRPCConnectionCache *defaults.ConnectionCache, enterpriseJobs jobutil.EnterpriseJobs) SearchClient {
	return &searchClient{
		logger:                      logger,
		db:                          db,
		zoekt:                       zoektStreamer,
		searcherURLs:                searcherURLs,
		searcherGRPCConnectionCache: searcherGRPCConnectionCache,
		enterpriseJobs:              enterpriseJobs,
	}
}

type searchClient struct {
	logger                      log.Logger
	db                          database.DB
	zoekt                       zoekt.Streamer
	searcherURLs                *endpoint.Map
	searcherGRPCConnectionCache *defaults.ConnectionCache
	enterpriseJobs              jobutil.EnterpriseJobs
}

func (s *searchClient) Plan(
	ctx context.Context,
	version string,
	patternType *string,
	searchQuery string,
	searchMode search.Mode,
	protocol search.Protocol,
	settings *schema.Settings,
	sourcegraphDotComMode bool,
) (_ *search.Inputs, err error) {
	tr, ctx := trace.New(ctx, "NewSearchInputs", searchQuery)
	defer tr.FinishWithErr(&err)

	searchType, err := detectSearchType(version, patternType)
	if err != nil {
		return nil, err
	}
	searchType = overrideSearchType(searchQuery, searchType)

	if searchType == query.SearchTypeStructural && !conf.StructuralSearchEnabled() {
		return nil, errors.New("Structural search is disabled in the site configuration.")
	}

	// Beta: create a step to replace each context in the query with its repository query if any.
	searchContextsQueryEnabled := settings.ExperimentalFeatures != nil && getBoolPtr(settings.ExperimentalFeatures.SearchContextsQuery, true)
	substituteContextsStep := query.SubstituteSearchContexts(func(context string) (string, error) {
		sc, err := searchcontexts.ResolveSearchContextSpec(ctx, s.db, context)
		if err != nil {
			return "", err
		}
		tr.LazyPrintf("substitute query %s for context %s", sc.Query, context)
		return sc.Query, nil
	})

	var plan query.Plan
	plan, err = query.Pipeline(
		query.Init(searchQuery, searchType),
		query.With(searchContextsQueryEnabled, substituteContextsStep),
	)
	if err != nil {
		return nil, &QueryError{Query: searchQuery, Err: err}
	}
	tr.LazyPrintf("parsing done")

	inputs := &search.Inputs{
		Plan:                   plan,
		Query:                  plan.ToQ(),
		OriginalQuery:          searchQuery,
		SearchMode:             searchMode,
		UserSettings:           settings,
		OnSourcegraphDotCom:    sourcegraphDotComMode,
		Features:               ToFeatures(featureflag.FromContext(ctx), s.logger),
		PatternType:            searchType,
		Protocol:               protocol,
		SanitizeSearchPatterns: sanitizeSearchPatterns(ctx, s.db, s.logger), // Experimental: check site config to see if search sanitization is enabled
	}

	tr.LazyPrintf("Parsed query: %s", inputs.Query)

	return inputs, nil
}

func (s *searchClient) Execute(
	ctx context.Context,
	stream streaming.Sender,
	inputs *search.Inputs,
) (_ *search.Alert, err error) {
	tr, ctx := trace.New(ctx, "Execute", "")
	defer tr.FinishWithErr(&err)

	planJob, err := jobutil.NewPlanJob(inputs, inputs.Plan, s.enterpriseJobs)
	if err != nil {
		return nil, err
	}

	return planJob.Run(ctx, s.JobClients(), stream)
}

func (s *searchClient) JobClients() job.RuntimeClients {
	return job.RuntimeClients{
		Logger:                      s.logger,
		DB:                          s.db,
		Zoekt:                       s.zoekt,
		SearcherURLs:                s.searcherURLs,
		SearcherGRPCConnectionCache: s.searcherGRPCConnectionCache,
		Gitserver:                   gitserver.NewClient(),
	}
}

func sanitizeSearchPatterns(ctx context.Context, db database.DB, log log.Logger) []*regexp.Regexp {
	var sanitizePatterns []*regexp.Regexp
	c := conf.Get()
	if c.ExperimentalFeatures != nil && c.ExperimentalFeatures.SearchSanitization != nil {
		actr := actor.FromContext(ctx)
		if actr.IsInternal() {
			return []*regexp.Regexp{}
		}

		for _, pat := range c.ExperimentalFeatures.SearchSanitization.SanitizePatterns {
			if re, err := regexp.Compile(pat); err != nil {
				log.Warn("invalid regex pattern provided, ignoring")
			} else {
				sanitizePatterns = append(sanitizePatterns, re)
			}
		}

		user, err := actr.User(ctx, db.Users())
		if err != nil {
			log.Warn("search being run as invalid user")
			return sanitizePatterns
		}

		if user.SiteAdmin {
			return []*regexp.Regexp{}
		}

		if c.ExperimentalFeatures.SearchSanitization.OrgName != "" {
			orgStore := db.Orgs()
			userOrgs, err := orgStore.GetByUserID(ctx, user.ID)
			if err != nil {
				return sanitizePatterns
			}

			for _, org := range userOrgs {
				if org.Name == c.ExperimentalFeatures.SearchSanitization.OrgName {
					return []*regexp.Regexp{}
				}
			}
		}
	}
	return sanitizePatterns
}

type QueryError struct {
	Query string
	Err   error
}

func (e *QueryError) Error() string {
	return fmt.Sprintf("invalid query %q: %s", e.Query, e.Err)
}

func SearchTypeFromString(patternType string) (query.SearchType, error) {
	switch patternType {
	case "standard":
		return query.SearchTypeStandard, nil
	case "literal":
		return query.SearchTypeLiteral, nil
	case "regexp":
		return query.SearchTypeRegex, nil
	case "structural":
		return query.SearchTypeStructural, nil
	case "lucky":
		return query.SearchTypeLucky, nil
	case "keyword":
		return query.SearchTypeKeyword, nil
	default:
		return -1, errors.Errorf("unrecognized patternType %q", patternType)
	}
}

// detectSearchType returns the search type to perform. The search type derives
// from three sources: the version and patternType parameters passed to the
// search endpoint and the `patternType:` filter in the input query string which
// overrides the searchType, if present.
func detectSearchType(version string, patternType *string) (query.SearchType, error) {
	var searchType query.SearchType
	if patternType != nil {
		return SearchTypeFromString(*patternType)
	} else {
		switch version {
		case "V1":
			searchType = query.SearchTypeRegex
		case "V2":
			searchType = query.SearchTypeLiteral
		case "V3":
			searchType = query.SearchTypeStandard
		default:
			return -1, errors.Errorf("unrecognized version: want \"V1\", \"V2\", or \"V3\", got %q", version)
		}
	}
	return searchType, nil
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
		case "standard":
			searchType = query.SearchTypeStandard
		case "regex", "regexp":
			searchType = query.SearchTypeRegex
		case "literal":
			searchType = query.SearchTypeLiteral
		case "structural":
			searchType = query.SearchTypeStructural
		case "lucky":
			searchType = query.SearchTypeLucky
		case "keyword":
			searchType = query.SearchTypeKeyword
		}
	})
	return searchType
}

func ToFeatures(flagSet *featureflag.FlagSet, logger log.Logger) *search.Features {
	if flagSet == nil {
		flagSet = &featureflag.FlagSet{}
		metricFeatureFlagUnavailable.Inc()
		logger.Warn("search feature flags are not available")
	}

	return &search.Features{
		ContentBasedLangFilters: flagSet.GetBoolOr("search-content-based-lang-detection", false),
		CodeOwnershipSearch:     flagSet.GetBoolOr("search-ownership", false),
		HybridSearch:            flagSet.GetBoolOr("search-hybrid", true), // can remove flag in 4.5
		Ranking:                 flagSet.GetBoolOr("search-ranking", false),
		Debug:                   flagSet.GetBoolOr("search-debug", false),
	}
}

var metricFeatureFlagUnavailable = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_search_featureflag_unavailable",
	Help: "temporary counter to check if we have feature flag available in practice.",
})

func getBoolPtr(b *bool, def bool) bool {
	if b == nil {
		return def
	}
	return *b
}
