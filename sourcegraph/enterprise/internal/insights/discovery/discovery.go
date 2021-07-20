package discovery

import (
	"context"
	"time"

	"github.com/cockroachdb/errors"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/types"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"

	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"

	"github.com/sourcegraph/sourcegraph/internal/goroutine"

	"github.com/sourcegraph/sourcegraph/internal/insights"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
)

// SettingStore is a subset of the API exposed by the database.Settings() store.
type SettingStore interface {
	GetLatest(context.Context, api.SettingsSubject) (*api.Settings, error)
	GetLastestSchemaSettings(context.Context, api.SettingsSubject) (*schema.Settings, error)
}

// InsightFilterArgs contains arguments that will filter out insights when discovered if matched.
type InsightFilterArgs struct {
	Ids []string
}

// Discover uses the given settings store to look for insights in the global user settings.
func Discover(ctx context.Context, settingStore SettingStore, loader insights.Loader, args InsightFilterArgs) ([]insights.SearchInsight, error) {
	discovered, err := discoverAll(ctx, settingStore, loader)
	if err != nil {
		return []insights.SearchInsight{}, err
	}
	return applyFilters(discovered, args), nil
}

// discoverIntegrated will load any insights that are integrated (meaning backend capable) from the extensions settings
func discoverIntegrated(ctx context.Context, loader insights.Loader) ([]insights.SearchInsight, error) {
	return loader.LoadAll(ctx)
}

func discoverAll(ctx context.Context, settingStore SettingStore, loader insights.Loader) ([]insights.SearchInsight, error) {
	// Get latest Global user settings.
	subject := api.SettingsSubject{Site: true}
	globalSettingsRaw, err := settingStore.GetLatest(ctx, subject)
	if err != nil {
		return nil, err
	}
	globalSettings, err := parseUserSettings(globalSettingsRaw)
	if err != nil {
		return nil, err
	}
	results := convertFromBackendInsight(globalSettings.Insights)
	integrated, err := discoverIntegrated(ctx, loader)
	if err != nil {
		return nil, err
	}

	return append(results, integrated...), nil
}

// convertFromBackendInsight is an adapter method that will transform the 'backend' insight schema to the schema that is
// used by the extensions on the frontend, and will be used in the future. As soon as the backend and frontend are fully integrated these
// 'backend' insights will be deprecated.
func convertFromBackendInsight(backendInsights []*schema.Insight) []insights.SearchInsight {
	converted := make([]insights.SearchInsight, 0)
	for _, backendInsight := range backendInsights {
		var temp insights.SearchInsight
		temp.Title = backendInsight.Title
		temp.Description = backendInsight.Description
		for _, series := range backendInsight.Series {
			temp.Series = append(temp.Series, insights.TimeSeries{
				Name:  series.Label,
				Query: series.Search,
			})
		}
		temp.ID = backendInsight.Id
		converted = append(converted, temp)
	}

	return converted
}

func parseUserSettings(settings *api.Settings) (*schema.Settings, error) {
	if settings == nil {
		// Settings have never been saved for this subject; equivalent to `{}`.
		return &schema.Settings{}, nil
	}
	var v schema.Settings
	if err := jsonc.Unmarshal(settings.Contents, &v); err != nil {
		return nil, err
	}
	return &v, nil
}

// applyFilters will apply any filters defined as arguments serially and return the intersection.
func applyFilters(total []insights.SearchInsight, args InsightFilterArgs) []insights.SearchInsight {
	filtered := total

	if len(args.Ids) > 0 {
		filtered = filterByIds(args.Ids, total)
	}

	return filtered
}

func filterByIds(ids []string, insight []insights.SearchInsight) []insights.SearchInsight {
	filtered := make([]insights.SearchInsight, 0)
	keys := make(map[string]bool)
	for _, id := range ids {
		keys[id] = true
	}

	for _, searchInsight := range insight {
		if _, ok := keys[searchInsight.ID]; ok {
			filtered = append(filtered, searchInsight)
		}
	}
	return filtered
}

type settingMigrator struct {
	base     dbutil.DB
	insights dbutil.DB
}

// NewMigrateSettingInsightsJob will migrate insights from settings into the database. This is a job that will be
// deprecated as soon as this functionality is available over an API.
func NewMigrateSettingInsightsJob(ctx context.Context, base dbutil.DB, insights dbutil.DB) goroutine.BackgroundRoutine {
	interval := time.Hour
	m := settingMigrator{
		base:     base,
		insights: insights,
	}

	return goroutine.NewPeriodicGoroutine(ctx, interval,
		goroutine.NewHandlerWithErrorMessage("insight_setting_migrator", m.migrate))
}

func (m *settingMigrator) migrate(ctx context.Context) error {
	insightStore := store.NewInsightStore(m.insights)
	loader := insights.NewLoader(m.base)

	discovered, err := discoverIntegrated(ctx, loader)
	if err != nil {
		return err
	}

	var count, skipped, errors int
	for _, d := range discovered {
		if d.ID == "" {
			// we need a unique ID, and if for some reason this insight doesn't have one, it can't be migrated.
			skipped++
			continue
		}
		results, err := insightStore.Get(ctx, store.InsightQueryArgs{
			UniqueID: d.ID,
		})
		if err != nil {
			return err
		}
		if len(results) != 0 {
			// this insight has already been ingested, so let's skip it. Technically this insight could have been edited
			// but for now we are going to ignore any edits to display settings.
			skipped++
			continue
		}

		err = migrateSeries(ctx, insightStore, d)
		if err != nil {
			// we can't do anything about errors, so we will just skip it and log it
			errors++
			log15.Error("error while migrating insight", "error", err)
		}
		count++
	}
	log15.Info("insights settings migration complete", "count", count, "skipped", skipped, "errors", errors)
	return nil
}

// migrateSeries will attempt to take an insight defined in Sourcegraph settings and migrate it to the database.
func migrateSeries(ctx context.Context, insightStore *store.InsightStore, from insights.SearchInsight) (err error) {
	tx, err := insightStore.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Store.Done(err) }()

	log15.Info("attempting to migrate insight", "unique_id", from.ID)
	series := make([]types.InsightSeries, len(from.Series))
	metadata := make([]types.InsightViewSeriesMetadata, len(from.Series))

	for i, timeSeries := range from.Series {
		temp := types.InsightSeries{
			SeriesID:              Encode(timeSeries),
			Query:                 timeSeries.Query,
			RecordingIntervalDays: 1,
		}
		result, err := tx.CreateSeries(ctx, temp)
		if err != nil {
			return errors.Wrapf(err, "unable to migrate insight unique_id: %s series_id: %s", from.ID, temp.SeriesID)
		}
		series[i] = result

		metadata[i] = types.InsightViewSeriesMetadata{
			Label:  timeSeries.Name,
			Stroke: timeSeries.Stroke,
		}
	}

	view := types.InsightView{
		Title:       from.Title,
		Description: from.Description,
		UniqueID:    from.ID,
	}

	view, err = tx.CreateView(ctx, view)
	if err != nil {
		return errors.Wrapf(err, "unable to migrate insight unique_id: %s", from.ID)
	}

	for i, insightSeries := range series {
		err := tx.AttachSeriesToView(ctx, insightSeries, view, metadata[i])
		if err != nil {
			return errors.Wrapf(err, "unable to migrate insight unique_id: %s", from.ID)
		}
	}
	return nil
}
