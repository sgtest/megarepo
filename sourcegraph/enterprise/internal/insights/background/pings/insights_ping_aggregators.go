package pings

import (
	"context"
	"fmt"

	insightTypes "github.com/sourcegraph/sourcegraph/enterprise/internal/insights/types"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func (e *InsightsPingEmitter) GetTotalCountByViewType(ctx context.Context) (_ []types.InsightViewsCountPing, err error) {
	rows, err := e.insightsDb.QueryContext(ctx, insightViewTotalCountQuery)
	if err != nil {
		return []types.InsightViewsCountPing{}, err
	}
	defer func() { err = rows.Close() }()

	results := make([]types.InsightViewsCountPing, 0)
	for rows.Next() {
		stats := types.InsightViewsCountPing{}
		if err := rows.Scan(&stats.ViewType, &stats.TotalCount); err != nil {
			return []types.InsightViewsCountPing{}, err
		}
		results = append(results, stats)
	}

	return results, nil
}

func (e *InsightsPingEmitter) GetTotalCountByViewSeriesType(ctx context.Context) (_ []types.InsightViewSeriesCountPing, err error) {
	q := fmt.Sprintf(insightViewSeriesTotalCountQuery, pingSeriesType)
	rows, err := e.insightsDb.QueryContext(ctx, q)
	if err != nil {
		return []types.InsightViewSeriesCountPing{}, err
	}
	defer func() { err = rows.Close() }()

	results := make([]types.InsightViewSeriesCountPing, 0)
	for rows.Next() {
		stats := types.InsightViewSeriesCountPing{}
		if err := rows.Scan(&stats.ViewType, &stats.GenerationType, &stats.TotalCount); err != nil {
			return []types.InsightViewSeriesCountPing{}, err
		}
		results = append(results, stats)
	}

	return results, nil
}

func (e *InsightsPingEmitter) GetTotalCountBySeriesType(ctx context.Context) (_ []types.InsightSeriesCountPing, err error) {
	q := fmt.Sprintf(insightSeriesTotalCountQuery, pingSeriesType)
	rows, err := e.insightsDb.QueryContext(ctx, q)
	if err != nil {
		return []types.InsightSeriesCountPing{}, err
	}
	defer func() { err = rows.Close() }()

	results := make([]types.InsightSeriesCountPing, 0)
	for rows.Next() {
		stats := types.InsightSeriesCountPing{}
		if err := rows.Scan(&stats.GenerationType, &stats.TotalCount); err != nil {
			return []types.InsightSeriesCountPing{}, err
		}
		results = append(results, stats)
	}

	return results, nil
}

func (e *InsightsPingEmitter) GetIntervalCounts(ctx context.Context) (_ []types.InsightTimeIntervalPing, err error) {
	rows, err := e.insightsDb.QueryContext(ctx, insightIntervalCountsQuery)
	if err != nil {
		return []types.InsightTimeIntervalPing{}, err
	}
	defer func() { err = rows.Close() }()

	results := make([]types.InsightTimeIntervalPing, 0)
	for rows.Next() {
		var count, intervalValue int
		var intervalUnit insightTypes.IntervalUnit
		if err := rows.Scan(&count, &intervalValue, &intervalUnit); err != nil {
			return []types.InsightTimeIntervalPing{}, err
		}

		results = append(results, types.InsightTimeIntervalPing{IntervalDays: getDays(intervalValue, intervalUnit), TotalCount: count})
	}
	regroupedResults := regroupIntervalCounts(results)
	return regroupedResults, nil
}

func (e *InsightsPingEmitter) GetOrgVisibleInsightCounts(ctx context.Context) (_ []types.OrgVisibleInsightPing, err error) {
	rows, err := e.insightsDb.QueryContext(ctx, orgVisibleInsightCountsQuery)
	if err != nil {
		return []types.OrgVisibleInsightPing{}, err
	}
	defer func() { err = rows.Close() }()

	results := make([]types.OrgVisibleInsightPing, 0)
	for rows.Next() {
		var count int
		var presentationType insightTypes.PresentationType
		if err := rows.Scan(&presentationType, &count); err != nil {
			return []types.OrgVisibleInsightPing{}, err
		}

		if presentationType == insightTypes.Line {
			results = append(results, types.OrgVisibleInsightPing{Type: "search", TotalCount: count})
		} else {
			results = append(results, types.OrgVisibleInsightPing{Type: "lang-stats", TotalCount: count})
		}
	}
	return results, nil
}

func getDays(intervalValue int, intervalUnit insightTypes.IntervalUnit) int {
	switch intervalUnit {
	case insightTypes.Month:
		return intervalValue * 30
	case insightTypes.Week:
		return intervalValue * 7
	case insightTypes.Day:
		return intervalValue
	case insightTypes.Hour:
		// We can't return anything more granular than 1 day.
		return 0
	}
	return 0
}

// This combines any groups of interval counts that have the same number of days.
// Example: A group that had unit MONTH and value 1 alongside a group that had unit DAY and value 30.
func regroupIntervalCounts(fromGroups []types.InsightTimeIntervalPing) []types.InsightTimeIntervalPing {
	groupByDays := make(map[int]int)
	newGroups := make([]types.InsightTimeIntervalPing, 0)

	for _, g := range fromGroups {
		groupByDays[g.IntervalDays] += g.TotalCount
	}
	for days, count := range groupByDays {
		newGroups = append(newGroups, types.InsightTimeIntervalPing{IntervalDays: days, TotalCount: count})
	}
	return newGroups
}

const pingSeriesType = `
CONCAT(
   CASE WHEN ((generation_method = 'search' or generation_method = 'search-compute') and generated_from_capture_groups) THEN 'capture-groups' ELSE generation_method END,
    '::',
   CASE WHEN (cardinality(repositories) = 0 or repositories is null) THEN 'global' ELSE 'scoped' END,
    '::',
   CASE WHEN (just_in_time = true) THEN 'jit' ELSE 'recorded' END
    ) as ping_series_type
`

const insightViewSeriesTotalCountQuery = `
SELECT presentation_type,
       %s,
       COUNT(*)
FROM insight_series
         JOIN insight_view_series ivs ON insight_series.id = ivs.insight_series_id
         JOIN insight_view iv ON ivs.insight_view_id = iv.id
WHERE deleted_at IS NULL
GROUP BY presentation_type, ping_series_type;
`

const insightSeriesTotalCountQuery = `
SELECT %s,
       COUNT(*)
FROM insight_series
WHERE deleted_at IS NULL
GROUP BY ping_series_type;
`

const insightViewTotalCountQuery = `
SELECT presentation_type, COUNT(*)
FROM insight_view
GROUP BY presentation_type;
`

const insightIntervalCountsQuery = `
SELECT COUNT(DISTINCT(ivs.insight_view_id)), series.sample_interval_value, series.sample_interval_unit FROM insight_series AS series
JOIN insight_view_series AS ivs ON series.id = ivs.insight_series_id
WHERE series.sample_interval_value != 0
	AND series.sample_interval_value IS NOT NULL
	AND series.sample_interval_unit IS NOT NULL
GROUP BY series.sample_interval_value, series.sample_interval_unit;
`

const orgVisibleInsightCountsQuery = `
SELECT iv.presentation_type, COUNT(iv.presentation_type) FROM insight_view AS iv
JOIN insight_view_grants AS ivg ON iv.id = ivg.insight_view_id
WHERE ivg.org_id IS NOT NULL
GROUP BY iv.presentation_type;
`
