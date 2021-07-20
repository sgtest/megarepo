package types

import (
	"time"
)

// InsightViewSeries is an abstraction of a complete Code Insight. This type materializes a view with any associated series.
type InsightViewSeries struct {
	UniqueID              string
	SeriesID              string
	Title                 string
	Description           string
	Query                 string
	CreatedAt             time.Time
	OldestHistoricalAt    time.Time
	LastRecordedAt        time.Time
	NextRecordingAfter    time.Time
	RecordingIntervalDays int
	Label                 string
	Stroke                string
}

// InsightViewSeriesMetadata contains metadata about a viewable insight series such as render properties.
type InsightViewSeriesMetadata struct {
	Label  string
	Stroke string
}

// InsightView is a single insight view that may or may not have any associated series.
type InsightView struct {
	ID          int
	Title       string
	Description string
	UniqueID    string
}

// InsightSeries is a single data series for a Code Insight. This contains some metadata about the data series, as well
// as its unique series ID.
type InsightSeries struct {
	ID                    int
	SeriesID              string
	Query                 string
	CreatedAt             time.Time
	OldestHistoricalAt    time.Time
	LastRecordedAt        time.Time
	NextRecordingAfter    time.Time
	RecordingIntervalDays int
}
