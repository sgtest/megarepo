package sqltemplate

// SQLite is an implementation of Dialect for the SQLite DMBS.
var SQLite = sqlite{
	rowLockingClauseAll: false,
}

var _ Dialect = SQLite

type sqlite struct {
	// See:
	//	https://www.sqlite.org/lang_keywords.html
	standardIdent
	rowLockingClauseAll
}
