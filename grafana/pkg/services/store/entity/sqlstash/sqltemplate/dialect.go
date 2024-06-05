package sqltemplate

import (
	"errors"
	"strconv"
	"strings"
)

// Dialect-agnostic errors.
var (
	ErrEmptyIdent              = errors.New("empty identifier")
	ErrInvalidRowLockingClause = errors.New("invalid row-locking clause")
)

// Dialect should be added to the data types passed to SQL templates to
// provide methods that deal with SQL implementation-specific traits. It can be
// embedded for ease of use, or with a named struct field if any of its methods
// would clash with other struct field names.
type Dialect interface {
	// Name identifies the Dialect. Note that a Dialect may be common to more
	// than one DBMS (e.g. "postgres" is common to PostgreSQL and to
	// CockroachDB), while we can maintain different Dialects for the same DBMS
	// but different versions (e.g. "mysql5" and "mysql8").
	Name() string

	// Ident returns the given string quoted in a way that is suitable to be
	// used as an identifier. Database names, schema names, table names, column
	// names are all examples of identifiers.
	Ident(string) (string, error)

	// ArgPlaceholder returns a safe argument suitable to be used in a SQL
	// prepared statement for the argNum-eth argument passed in execution
	// (starting at 1). The SQL92 Standard specifies the question mark ('?')
	// should be used in all cases, but some implementations differ.
	ArgPlaceholder(argNum int) string

	// SelectFor parses and returns the given row-locking clause for a SELECT
	// statement. If the clause is invalid it returns an error. Implementations
	// of this method should use ParseRowLockingClause.
	// Example:
	//
	//	SELECT *
	//		FROM mytab
	//		WHERE id = ?
	//		{{ .SelectFor "Update NoWait" }}; -- will be uppercased
	SelectFor(...string) (string, error)
}

// RowLockingClause represents a row-locking clause in a SELECT statement.
type RowLockingClause string

// Valid returns whether the given option is valid.
func (o RowLockingClause) Valid() bool {
	switch o {
	case SelectForShare, SelectForShareNoWait, SelectForShareSkipLocked,
		SelectForUpdate, SelectForUpdateNoWait, SelectForUpdateSkipLocked:
		return true
	}
	return false
}

// ParseRowLockingClause parses a RowLockingClause from the given strings. This
// should be used by implementations of Dialect to parse the input of the
// SelectFor method.
func ParseRowLockingClause(s ...string) (RowLockingClause, error) {
	opt := RowLockingClause(strings.ToUpper(strings.Join(s, " ")))
	if !opt.Valid() {
		return "", ErrInvalidRowLockingClause
	}

	return opt, nil
}

// Row-locking clause options.
const (
	SelectForShare            RowLockingClause = "SHARE"
	SelectForShareNoWait      RowLockingClause = "SHARE NOWAIT"
	SelectForShareSkipLocked  RowLockingClause = "SHARE SKIP LOCKED"
	SelectForUpdate           RowLockingClause = "UPDATE"
	SelectForUpdateNoWait     RowLockingClause = "UPDATE NOWAIT"
	SelectForUpdateSkipLocked RowLockingClause = "UPDATE SKIP LOCKED"
)

type rowLockingClauseMap map[RowLockingClause]RowLockingClause

func (rlc rowLockingClauseMap) SelectFor(s ...string) (string, error) {
	// all implementations should err on invalid input, otherwise we would just
	// be hiding the error until we change the dialect
	o, err := ParseRowLockingClause(s...)
	if err != nil {
		return "", err
	}

	var ret string
	if len(rlc) > 0 {
		ret = "FOR " + string(rlc[o])
	}

	return ret, nil
}

var rowLockingClauseAll = rowLockingClauseMap{
	SelectForShare:            SelectForShare,
	SelectForShareNoWait:      SelectForShareNoWait,
	SelectForShareSkipLocked:  SelectForShareSkipLocked,
	SelectForUpdate:           SelectForUpdate,
	SelectForUpdateNoWait:     SelectForUpdateNoWait,
	SelectForUpdateSkipLocked: SelectForUpdateSkipLocked,
}

// standardIdent provides standard SQL escaping of identifiers.
type standardIdent struct{}

func (standardIdent) Ident(s string) (string, error) {
	if s == "" {
		return "", ErrEmptyIdent
	}
	return `"` + strings.ReplaceAll(s, `"`, `""`) + `"`, nil
}

type argPlaceholderFunc func(int) string

func (f argPlaceholderFunc) ArgPlaceholder(argNum int) string {
	return f(argNum)
}

var (
	argFmtSQL92 = argPlaceholderFunc(func(int) string {
		return "?"
	})
	argFmtPositional = argPlaceholderFunc(func(argNum int) string {
		return "$" + strconv.Itoa(argNum)
	})
)

type name string

func (n name) Name() string {
	return string(n)
}
