package usagestats

import (
	"context"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func GetSiteUsageStats(ctx context.Context, db dbutil.DB, monthsOnly bool) (*types.SiteUsageStatistics, error) {
	summary, err := database.EventLogs(db).SiteUsage(ctx)
	if err != nil {
		return nil, err
	}

	stats := groupSiteUsageStats(summary, monthsOnly)
	return stats, nil
}

func groupSiteUsageStats(summary types.SiteUsageSummary, monthsOnly bool) *types.SiteUsageStatistics {
	stats := &types.SiteUsageStatistics{
		DAUs: []*types.SiteActivityPeriod{
			{
				StartTime:            summary.Day,
				UserCount:            summary.UniquesDay,
				RegisteredUserCount:  summary.RegisteredUniquesDay,
				AnonymousUserCount:   summary.UniquesDay - summary.RegisteredUniquesDay,
				IntegrationUserCount: summary.IntegrationUniquesDay,
			},
		},
		WAUs: []*types.SiteActivityPeriod{
			{
				StartTime:            summary.Week,
				UserCount:            summary.UniquesWeek,
				RegisteredUserCount:  summary.RegisteredUniquesWeek,
				AnonymousUserCount:   summary.UniquesWeek - summary.RegisteredUniquesWeek,
				IntegrationUserCount: summary.IntegrationUniquesWeek,
			},
		},
		MAUs: []*types.SiteActivityPeriod{
			{
				StartTime:            summary.Month,
				UserCount:            summary.UniquesMonth,
				RegisteredUserCount:  summary.RegisteredUniquesMonth,
				AnonymousUserCount:   summary.UniquesMonth - summary.RegisteredUniquesMonth,
				IntegrationUserCount: summary.IntegrationUniquesMonth,
			},
		},
	}

	if monthsOnly {
		stats.DAUs = []*types.SiteActivityPeriod{}
		stats.WAUs = []*types.SiteActivityPeriod{}
	}

	return stats
}

// GetAggregatedCodeIntelStats returns aggregated statistics for code intelligence usage.
func GetAggregatedCodeIntelStats(ctx context.Context, db dbutil.DB) (*types.NewCodeIntelUsageStatistics, error) {
	codeIntelEvents, err := database.EventLogs(db).AggregatedCodeIntelEvents(ctx)
	if err != nil {
		return nil, err
	} else if len(codeIntelEvents) == 0 {
		return nil, nil
	}
	stats := groupAggregatedCodeIntelStats(codeIntelEvents)

	pairs := []struct {
		fetch  func(ctx context.Context) (int, error)
		target **int32
	}{
		{database.EventLogs(db).CodeIntelligenceWAUs, &stats.WAUs},
		{database.EventLogs(db).CodeIntelligencePreciseWAUs, &stats.PreciseWAUs},
		{database.EventLogs(db).CodeIntelligenceSearchBasedWAUs, &stats.SearchBasedWAUs},
		{database.EventLogs(db).CodeIntelligenceCrossRepositoryWAUs, &stats.CrossRepositoryWAUs},
		{database.EventLogs(db).CodeIntelligencePreciseCrossRepositoryWAUs, &stats.PreciseCrossRepositoryWAUs},
		{database.EventLogs(db).CodeIntelligenceSearchBasedCrossRepositoryWAUs, &stats.SearchBasedCrossRepositoryWAUs},
	}

	for _, pair := range pairs {
		count, err := pair.fetch(ctx)
		if err != nil {
			return nil, err
		}

		v := int32(count)
		*pair.target = &v
	}

	withUploads, withoutUploads, err := database.EventLogs(db).CodeIntelligenceRepositoryCounts(ctx)
	if err != nil {
		return nil, err
	}
	stats.NumRepositoriesWithUploadRecords = int32Ptr(withUploads)
	stats.NumRepositoriesWithoutUploadRecords = int32Ptr(withoutUploads)

	return stats, nil
}

var actionMap = map[string]types.CodeIntelAction{
	"codeintel.lsifHover":               types.HoverAction,
	"codeintel.searchHover":             types.HoverAction,
	"codeintel.lsifDefinitions":         types.DefinitionsAction,
	"codeintel.lsifDefinitions.xrepo":   types.DefinitionsAction,
	"codeintel.searchDefinitions":       types.DefinitionsAction,
	"codeintel.searchDefinitions.xrepo": types.DefinitionsAction,
	"codeintel.lsifReferences":          types.ReferencesAction,
	"codeintel.lsifReferences.xrepo":    types.ReferencesAction,
	"codeintel.searchReferences":        types.ReferencesAction,
	"codeintel.searchReferences.xrepo":  types.ReferencesAction,
}

var sourceMap = map[string]types.CodeIntelSource{
	"codeintel.lsifHover":               types.PreciseSource,
	"codeintel.lsifDefinitions":         types.PreciseSource,
	"codeintel.lsifDefinitions.xrepo":   types.PreciseSource,
	"codeintel.lsifReferences":          types.PreciseSource,
	"codeintel.lsifReferences.xrepo":    types.PreciseSource,
	"codeintel.searchHover":             types.SearchSource,
	"codeintel.searchDefinitions":       types.SearchSource,
	"codeintel.searchDefinitions.xrepo": types.SearchSource,
	"codeintel.searchReferences":        types.SearchSource,
	"codeintel.searchReferences.xrepo":  types.SearchSource,
}

func groupAggregatedCodeIntelStats(rawEvents []types.CodeIntelAggregatedEvent) *types.NewCodeIntelUsageStatistics {
	var eventSummaries []types.CodeIntelEventSummary
	for _, event := range rawEvents {
		languageID := ""
		if event.LanguageID != nil {
			languageID = *event.LanguageID
		}

		eventSummaries = append(eventSummaries, types.CodeIntelEventSummary{
			Action:          actionMap[event.Name],
			Source:          sourceMap[event.Name],
			LanguageID:      languageID,
			CrossRepository: strings.HasSuffix(event.Name, ".xrepo"),
			WAUs:            event.UniquesWeek,
			TotalActions:    event.TotalWeek,
		})
	}

	return &types.NewCodeIntelUsageStatistics{
		StartOfWeek:    rawEvents[0].Week,
		EventSummaries: eventSummaries,
	}
}

// GetAggregatedSearchStats queries the database for search usage and returns
// the aggregates statistics in the format of our BigQuery schema.
func GetAggregatedSearchStats(ctx context.Context, db dbutil.DB) (*types.SearchUsageStatistics, error) {
	events, err := database.EventLogs(db).AggregatedSearchEvents(ctx, time.Now().UTC())
	if err != nil {
		return nil, err
	}

	return groupAggregatedSearchStats(events), nil
}

// groupAggregatedSearchStats takes a set of input events (originating from
// Sourcegraph's Postgres table) and returns a SearchUsageStatistics data type
// that ends up being stored in BigQuery. SearchUsageStatistics corresponds to
// the target DB schema.
func groupAggregatedSearchStats(events []types.SearchAggregatedEvent) *types.SearchUsageStatistics {
	searchUsageStats := &types.SearchUsageStatistics{
		Daily:   []*types.SearchUsagePeriod{newSearchEventPeriod()},
		Weekly:  []*types.SearchUsagePeriod{newSearchEventPeriod()},
		Monthly: []*types.SearchUsagePeriod{newSearchEventPeriod()},
	}

	// Iterate over events, updating searchUsageStats for each event
	for _, event := range events {
		populateSearchEventStatistics(event, searchUsageStats)
		populateSearchFilterCountStatistics(event, searchUsageStats)
	}

	return searchUsageStats
}

// utility functions that resolve a SearchEventStatistics value for a given event name for some SearchUsagePeriod.
var searchLatencyExtractors = map[string]func(p *types.SearchUsagePeriod) *types.SearchEventStatistics{
	"search.latencies.literal":    func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Literal },
	"search.latencies.regexp":     func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Regexp },
	"search.latencies.structural": func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Structural },
	"search.latencies.file":       func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.File },
	"search.latencies.repo":       func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Repo },
	"search.latencies.diff":       func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Diff },
	"search.latencies.commit":     func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Commit },
	"search.latencies.symbol":     func(p *types.SearchUsagePeriod) *types.SearchEventStatistics { return p.Symbol },
}

var searchFilterCountExtractors = map[string]func(p *types.SearchUsagePeriod) *types.SearchCountStatistics{
	"count_or":                          func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.OperatorOr },
	"count_and":                         func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.OperatorAnd },
	"count_not":                         func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.OperatorNot },
	"count_select_repo":                 func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.SelectRepo },
	"count_select_file":                 func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.SelectFile },
	"count_select_content":              func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.SelectContent },
	"count_select_symbol":               func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.SelectSymbol },
	"count_select_commit_diff_added":    func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.SelectCommitDiffAdded },
	"count_select_commit_diff_removed":  func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.SelectCommitDiffRemoved },
	"count_repo_contains":               func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.RepoContains },
	"count_repo_contains_file":          func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.RepoContainsFile },
	"count_repo_contains_content":       func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.RepoContainsContent },
	"count_repo_contains_commit_after":  func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.RepoContainsCommitAfter },
	"count_count_all":                   func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.CountAll },
	"count_non_global_context":          func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.NonGlobalContext },
	"count_only_patterns":               func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.OnlyPatterns },
	"count_only_patterns_three_or_more": func(p *types.SearchUsagePeriod) *types.SearchCountStatistics { return p.OnlyPatternsThreeOrMore },
}

// populateSearchEventStatistics is a side-effecting function that populates the
// `statistics` object. The `statistics` event value is our target output type.
//
// Overview how it works:
// (1) To populate the `statistics` object, we expect an event to have a supported event.Name.
//
// (2) Create a SearchUsagePeriod target object based on the event's period (i.e., Month, Week, Day).
//
// (3) Use the SearchUsagePeriod object as an argument for the utility functions
// above, to get a handle on the (currently zero-valued) SearchEventStatistics
// value that it contains that corresponds to that event type.
//
// (4) Populate that SearchEventStatistics object in the SearchUsagePeriod object with usage stats (latencies, etc).
func populateSearchEventStatistics(event types.SearchAggregatedEvent, statistics *types.SearchUsageStatistics) {
	extractor, ok := searchLatencyExtractors[event.Name]
	if !ok {
		return
	}

	makeLatencies := func(values []float64) *types.SearchEventLatencies {
		for len(values) < 3 {
			// If event logs didn't have samples, add zero values
			values = append(values, 0)
		}

		return &types.SearchEventLatencies{P50: values[0], P90: values[1], P99: values[2]}
	}

	statistics.Monthly[0].StartTime = event.Month
	month := extractor(statistics.Monthly[0])
	month.EventsCount = &event.TotalMonth
	month.UserCount = &event.UniquesMonth
	month.EventLatencies = makeLatencies(event.LatenciesMonth)

	statistics.Weekly[0].StartTime = event.Week
	week := extractor(statistics.Weekly[0])
	week.EventsCount = &event.TotalWeek
	week.UserCount = &event.UniquesWeek
	week.EventLatencies = makeLatencies(event.LatenciesWeek)

	statistics.Daily[0].StartTime = event.Day
	day := extractor(statistics.Daily[0])
	day.EventsCount = &event.TotalDay
	day.UserCount = &event.UniquesDay
	day.EventLatencies = makeLatencies(event.LatenciesDay)
}

func populateSearchFilterCountStatistics(event types.SearchAggregatedEvent, statistics *types.SearchUsageStatistics) {
	extractor, ok := searchFilterCountExtractors[event.Name]
	if !ok {
		return
	}

	statistics.Monthly[0].StartTime = event.Month
	month := extractor(statistics.Monthly[0])
	month.EventsCount = &event.TotalMonth
	month.UserCount = &event.UniquesMonth

	statistics.Weekly[0].StartTime = event.Week
	week := extractor(statistics.Weekly[0])
	week.EventsCount = &event.TotalMonth
	week.UserCount = &event.UniquesMonth

	statistics.Daily[0].StartTime = event.Day
	day := extractor(statistics.Daily[0])
	day.EventsCount = &event.TotalMonth
	day.UserCount = &event.UniquesMonth
}

func newSearchEventPeriod() *types.SearchUsagePeriod {
	return &types.SearchUsagePeriod{
		Literal:    newSearchEventStatistics(),
		Regexp:     newSearchEventStatistics(),
		Structural: newSearchEventStatistics(),
		File:       newSearchEventStatistics(),
		Repo:       newSearchEventStatistics(),
		Diff:       newSearchEventStatistics(),
		Commit:     newSearchEventStatistics(),
		Symbol:     newSearchEventStatistics(),

		// Counts of search query attributes. Ref: RFC 384.
		OperatorOr:              newSearchCountStatistics(),
		OperatorAnd:             newSearchCountStatistics(),
		OperatorNot:             newSearchCountStatistics(),
		SelectRepo:              newSearchCountStatistics(),
		SelectFile:              newSearchCountStatistics(),
		SelectContent:           newSearchCountStatistics(),
		SelectSymbol:            newSearchCountStatistics(),
		SelectCommitDiffAdded:   newSearchCountStatistics(),
		SelectCommitDiffRemoved: newSearchCountStatistics(),
		RepoContains:            newSearchCountStatistics(),
		RepoContainsFile:        newSearchCountStatistics(),
		RepoContainsContent:     newSearchCountStatistics(),
		RepoContainsCommitAfter: newSearchCountStatistics(),
		CountAll:                newSearchCountStatistics(),
		NonGlobalContext:        newSearchCountStatistics(),
		OnlyPatterns:            newSearchCountStatistics(),
		OnlyPatternsThreeOrMore: newSearchCountStatistics(),

		// DEPRECATED.
		Case:               newSearchCountStatistics(),
		Committer:          newSearchCountStatistics(),
		Lang:               newSearchCountStatistics(),
		Fork:               newSearchCountStatistics(),
		Archived:           newSearchCountStatistics(),
		Count:              newSearchCountStatistics(),
		Timeout:            newSearchCountStatistics(),
		Content:            newSearchCountStatistics(),
		Before:             newSearchCountStatistics(),
		After:              newSearchCountStatistics(),
		Author:             newSearchCountStatistics(),
		Message:            newSearchCountStatistics(),
		Index:              newSearchCountStatistics(),
		Repogroup:          newSearchCountStatistics(),
		Repohasfile:        newSearchCountStatistics(),
		Repohascommitafter: newSearchCountStatistics(),
		PatternType:        newSearchCountStatistics(),
		Type:               newSearchCountStatistics(),
		SearchModes:        newSearchModeUsageStatistics(),
	}
}

func newSearchEventStatistics() *types.SearchEventStatistics {
	return &types.SearchEventStatistics{EventLatencies: &types.SearchEventLatencies{}}
}

func newSearchCountStatistics() *types.SearchCountStatistics {
	return &types.SearchCountStatistics{}
}

func newSearchModeUsageStatistics() *types.SearchModeUsageStatistics {
	return &types.SearchModeUsageStatistics{Interactive: &types.SearchCountStatistics{}, PlainText: &types.SearchCountStatistics{}}
}

func int32Ptr(v int) *int32 {
	v32 := int32(v)
	return &v32
}
