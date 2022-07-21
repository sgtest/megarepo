package oobmigration

import "sort"

type MigrationInterrupt struct {
	Version      Version
	MigrationIDs []int
}

// ScheduleMigrationInterrupts returns the set of versions during an instance upgrade that
// have out-of-band migration completion requirements.
func ScheduleMigrationInterrupts() ([]MigrationInterrupt, error) {
	return scheduleMigrationInterrupts(yamlMigrations)
}

func scheduleMigrationInterrupts(migrations []yamlMigration) ([]MigrationInterrupt, error) {
	type migrationInterval struct {
		id         int
		introduced Version
		deprecated Version
	}

	// First, extract the intervals on which the given out of band migrations are defined. If
	// the interval hasn't been deprecated, it's still "open" and does not need to complete for
	// the instance upgrade operation to be successful.

	intervals := make([]migrationInterval, 0, len(migrations))
	for _, m := range migrations {
		if m.DeprecatedVersionMajor == nil {
			continue
		}

		intervals = append(intervals, migrationInterval{
			m.ID,
			Version{m.IntroducedVersionMajor, m.IntroducedVersionMinor},
			Version{*m.DeprecatedVersionMajor, *m.DeprecatedVersionMinor},
		})
	}

	// Choose a minimal set of versions that intersect all migration intervals. These will be the
	// points in the upgrade where we need to wait for an out of band migration to finish before
	// proceeding to subsequent versions.
	//
	// The following greedy algorithm chooses the optimal number of versions with a single scan
	// over the intervals:
	//
	//   (1) Order intervals by increasing upper bound
	//   (2) For each interval, choose a new version equal to the interval's upper bound if
	//       no previously chosen version falls within the interval.

	sort.Slice(intervals, func(i, j int) bool {
		return compareVersions(intervals[i].deprecated, intervals[j].deprecated) == VersionOrderBefore
	})

	points := make([]Version, 0, len(intervals))
	for _, interval := range intervals {
		if len(points) == 0 || compareVersions(points[len(points)-1], interval.introduced) == VersionOrderBefore {
			points = append(points, interval.deprecated)
		}
	}

	// Finally, we reconstruct the return value, which pairs each of our chosen versions with the
	// set of migrations that need to finish prior to continuing the upgrade process. When an interval
	// contains multiple chosen versions, we add it only to the largest version so that we delay
	// completion as long as possible (hence the reversal of the points slice).

	coveringSet := make(map[Version][]int, len(intervals))

	for i, j := 0, len(points)-1; i < j; i, j = i+1, j-1 {
		points[i], points[j] = points[j], points[i]
	}

outer:
	for _, interval := range intervals {
		for _, point := range points {
			// check for intersection
			if pointIntersectsInterval(interval.introduced, interval.deprecated, point) {
				coveringSet[point] = append(coveringSet[point], interval.id)
				continue outer
			}
		}

		panic("unreachable: input interval not covered in output")
	}

	interupts := make([]MigrationInterrupt, 0, len(coveringSet))
	for version, ids := range coveringSet {
		sort.Ints(ids)
		interupts = append(interupts, MigrationInterrupt{version, ids})
	}
	sort.Slice(interupts, func(i, j int) bool {
		return compareVersions(interupts[i].Version, interupts[j].Version) == VersionOrderBefore
	})

	return interupts, nil
}
