package main

import (
	"log"
	"os"
	"strconv"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/env"
)

var (
	rawFrontendURL, _                   = os.LookupEnv("SRC_FRONTEND_INTERNAL")
	rawResetInterval                    = env.Get("PRECISE_CODE_INTEL_RESET_INTERVAL", "1m", "How often to reset stalled indexes.")
	rawIndexerPollInterval              = env.Get("PRECISE_CODE_INTEL_INDEXER_POLL_INTERVAL", "1s", "Interval between queries to the index queue.")
	rawIndexabilityUpdaterInterval      = env.Get("PRECISE_CODE_INTEL_INDEXABILITY_UPDATER_INTERVAL", "30m", "Interval between scheduled indexability updates.")
	rawSchedulerInterval                = env.Get("PRECISE_CODE_INTEL_SCHEDULER_INTERVAL", "30m", "Interval between scheduled index updates.")
	rawIndexBatchSize                   = env.Get("PRECISE_CODE_INTEL_INDEX_BATCH_SIZE", "25", "Number of indexable repos to consider on each index scheduler update.")
	rawIndexMinimumTimeSinceLastEnqueue = env.Get("PRECISE_CODE_INTEL_INDEX_MINIMUM_TIME_SINCE_LAST_ENQUEUE", "24h", "Interval between indexing runs of the same repo.")
	rawIndexMinimumSearchCount          = env.Get("PRECISE_CODE_INTEL_INDEX_MINIMUM_SEARCH_COUNT", "50", "Minimum number of search events to trigger indexing for a repo.")
	rawIndexMinimumPreciseCount         = env.Get("PRECISE_CODE_INTEL_INDEX_MINIMUM_PRECISE_COUNT", "0", "Minimum number of precise events to trigger indexing for a repo.")
	rawIndexMinimumSearchRatio          = env.Get("PRECISE_CODE_INTEL_INDEX_MINIMUM_SEARCH_RATIO", "50", "Minimum ratio of search events to total events to trigger indexing for a repo.")
)

// mustGet returns the non-empty version of the given raw value fatally logs on failure.
func mustGet(rawValue, name string) string {
	if rawValue == "" {
		log.Fatalf("invalid value %q for %s: no value supplied", rawValue, name)
	}

	return rawValue
}

// mustParseInt returns the integer version of the given raw value fatally logs on failure.
func mustParseInt(rawValue, name string) int {
	i, err := strconv.ParseInt(rawValue, 10, 64)
	if err != nil {
		log.Fatalf("invalid int %q for %s: %s", rawValue, name, err)
	}

	return int(i)
}

// mustParsePercent returns the integer percent (in range [0, 100]) version of the given raw
// value fatally logs on failure.
func mustParsePercent(rawValue, name string) int {
	p := mustParseInt(rawValue, name)
	if p < 0 || p > 100 {
		log.Fatalf("invalid percent %q for %s: must be 0 <= p <= 100", rawValue, name)
	}

	return p
}

// mustParseInterval returns the interval version of the given raw value fatally logs on failure.
func mustParseInterval(rawValue, name string) time.Duration {
	d, err := time.ParseDuration(rawValue)
	if err != nil {
		log.Fatalf("invalid duration %q for %s: %s", rawValue, name, err)
	}

	return d
}
