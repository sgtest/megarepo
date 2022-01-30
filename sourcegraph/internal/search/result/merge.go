package result

// Intersect performs a merge of match results, merging line matches for files
// contained in both result sets.
func Intersect(left, right []Match) []Match {
	rightMap := make(map[Key]Match, len(right))
	for _, r := range right {
		rightMap[r.Key()] = r
	}

	merged := left[:0]
	for _, l := range left {
		r := rightMap[l.Key()]
		if r == nil {
			continue
		}
		switch leftMatch := l.(type) {
		// key matches, so we know to convert to respective type
		case *FileMatch:
			leftMatch.AppendMatches(r.(*FileMatch))
		case *CommitMatch:
			leftMatch.AppendMatches(r.(*CommitMatch))
		}
		merged = append(merged, l)
	}
	return merged
}
