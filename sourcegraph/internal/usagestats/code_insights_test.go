package usagestats

import (
	"context"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestCodeInsightsUsageStatistics(t *testing.T) {
	ctx := context.Background()

	defer func() {
		timeNow = time.Now
	}()

	weekStart := time.Date(2021, 1, 25, 0, 0, 0, 0, time.UTC)
	now := time.Date(2021, 1, 28, 0, 0, 0, 0, time.UTC)
	mockTimeNow(now)

	dbtesting.SetupGlobalTestDB(t)

	_, err := dbconn.Global.Exec(`
		INSERT INTO event_logs
			(id, name, argument, url, user_id, anonymous_user_id, source, version, timestamp)
		VALUES
			(1, 'ViewInsights', '{}', '', 1, '420657f0-d443-4d16-ac7d-003d8cdc91ef', 'WEB', '3.23.0', $1::timestamp - interval '1 day'),
			(2, 'ViewInsights', '{}', '', 1, '420657f0-d443-4d16-ac7d-003d8cdc91ef', 'WEB', '3.23.0', $1::timestamp - interval '1 day'),
			(3, 'InsightAddition', '{"insightType": "searchInsights"}', '', 1, '420657f0-d443-4d16-ac7d-003d8cdc91ef', 'WEB', '3.23.0', $1::timestamp - interval '1 days'),
			(4, 'InsightAddition', '{"insightType": "codeStatsInsights"}', '', 2, '420657f0-d443-4d16-ac7d-003d8cdc19ac', 'WEB', '3.23.0', $1::timestamp - interval '1 day'),
			(5, 'InsightAddition', '{"insightType": "searchInsights"}', '', 2, '420657f0-d443-4d16-ac7d-003d8cdc19ac', 'WEB', '3.23.0', $1::timestamp - interval '1 day'),
			(6, 'InsightEdit', '{"insightType": "searchInsights"}', '', 2, '420657f0-d443-4d16-ac7d-003d8cdc19ac', 'WEB', '3.23.0', $1::timestamp - interval '8 days'),
			(7, 'InsightAddition', '{"insightType": "codeStatsInsights"}', '', 1, '420657f0-d443-4d16-ac7d-003d8cdc91ef', 'WEB', '3.23.0', $1::timestamp - interval '8 days')
	`, now)
	if err != nil {
		t.Fatal(err)
	}

	have, err := GetCodeInsightsUsageStatistics(ctx)
	if err != nil {
		t.Fatal(err)
	}

	zeroInt := int32(0)
	oneInt := int32(1)
	twoInt := int32(2)

	searchInsightsType := "searchInsights"
	codeStatsInsightsType := "codeStatsInsights"

	usageStatisticsByInsight := []*types.InsightUsageStatistics{
		{
			InsightType:      &searchInsightsType,
			Additions:        &twoInt,
			Edits:            &oneInt,
			Removals:         &zeroInt,
			Hovers:           &zeroInt,
			UICustomizations: &zeroInt,
			DataPointClicks:  &zeroInt,
		},
		{
			InsightType:      &codeStatsInsightsType,
			Additions:        &twoInt,
			Edits:            &zeroInt,
			Removals:         &zeroInt,
			Hovers:           &zeroInt,
			UICustomizations: &zeroInt,
			DataPointClicks:  &zeroInt,
		},
	}

	want := &types.CodeInsightsUsageStatistics{
		UsageStatisticsByInsight:       usageStatisticsByInsight,
		InsightsPageViews:              &twoInt,
		InsightsUniquePageViews:        &oneInt,
		InsightConfigureClick:          &zeroInt,
		InsightAddMoreClick:            &zeroInt,
		WeekStart:                      weekStart,
		WeeklyInsightCreators:          &twoInt,
		WeeklyFirstTimeInsightCreators: &oneInt,
	}
	if diff := cmp.Diff(want, have); diff != "" {
		t.Fatal(diff)
	}
}
