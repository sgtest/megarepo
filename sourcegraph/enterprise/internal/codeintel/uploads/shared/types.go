package shared

import (
	"database/sql/driver"
	"encoding/json"
	"strconv"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/executor"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Upload struct {
	ID                int
	Commit            string
	Root              string
	VisibleAtTip      bool
	UploadedAt        time.Time
	State             string
	FailureMessage    *string
	StartedAt         *time.Time
	FinishedAt        *time.Time
	ProcessAfter      *time.Time
	NumResets         int
	NumFailures       int
	RepositoryID      int
	RepositoryName    string
	Indexer           string
	IndexerVersion    string
	NumParts          int
	UploadedParts     []int
	UploadSize        *int64
	UncompressedSize  *int64
	Rank              *int
	AssociatedIndexID *int
	ContentType       string
	ShouldReindex     bool
}

func (u Upload) RecordID() int {
	return u.ID
}

func (u Upload) RecordUID() string {
	return strconv.Itoa(u.ID)
}

// TODO - unify with Upload
// Dump is a subset of the lsif_uploads table (queried via the lsif_dumps_with_repository_name view)
// and stores only processed records.
type Dump struct {
	ID                int        `json:"id"`
	Commit            string     `json:"commit"`
	Root              string     `json:"root"`
	VisibleAtTip      bool       `json:"visibleAtTip"`
	UploadedAt        time.Time  `json:"uploadedAt"`
	State             string     `json:"state"`
	FailureMessage    *string    `json:"failureMessage"`
	StartedAt         *time.Time `json:"startedAt"`
	FinishedAt        *time.Time `json:"finishedAt"`
	ProcessAfter      *time.Time `json:"processAfter"`
	NumResets         int        `json:"numResets"`
	NumFailures       int        `json:"numFailures"`
	RepositoryID      int        `json:"repositoryId"`
	RepositoryName    string     `json:"repositoryName"`
	Indexer           string     `json:"indexer"`
	IndexerVersion    string     `json:"indexerVersion"`
	AssociatedIndexID *int       `json:"associatedIndex"`
}

type UploadLog struct {
	LogTimestamp      time.Time
	RecordDeletedAt   *time.Time
	UploadID          int
	Commit            string
	Root              string
	RepositoryID      int
	UploadedAt        time.Time
	Indexer           string
	IndexerVersion    *string
	UploadSize        *int
	AssociatedIndexID *int
	TransitionColumns []map[string]*string
	Reason            *string
	Operation         string
}

type Index struct {
	ID                 int                          `json:"id"`
	Commit             string                       `json:"commit"`
	QueuedAt           time.Time                    `json:"queuedAt"`
	State              string                       `json:"state"`
	FailureMessage     *string                      `json:"failureMessage"`
	StartedAt          *time.Time                   `json:"startedAt"`
	FinishedAt         *time.Time                   `json:"finishedAt"`
	ProcessAfter       *time.Time                   `json:"processAfter"`
	NumResets          int                          `json:"numResets"`
	NumFailures        int                          `json:"numFailures"`
	RepositoryID       int                          `json:"repositoryId"`
	LocalSteps         []string                     `json:"local_steps"`
	RepositoryName     string                       `json:"repositoryName"`
	DockerSteps        []DockerStep                 `json:"docker_steps"`
	Root               string                       `json:"root"`
	Indexer            string                       `json:"indexer"`
	IndexerArgs        []string                     `json:"indexer_args"` // TODO - convert this to `IndexCommand string`
	Outfile            string                       `json:"outfile"`
	ExecutionLogs      []executor.ExecutionLogEntry `json:"execution_logs"`
	Rank               *int                         `json:"placeInQueue"`
	AssociatedUploadID *int                         `json:"associatedUpload"`
	ShouldReindex      bool                         `json:"shouldReindex"`
	RequestedEnvVars   []string                     `json:"requestedEnvVars"`
}

func (i Index) RecordID() int {
	return i.ID
}

func (i Index) RecordUID() string {
	return strconv.Itoa(i.ID)
}

type DockerStep struct {
	Root     string   `json:"root"`
	Image    string   `json:"image"`
	Commands []string `json:"commands"`
}

func (s *DockerStep) Scan(value any) error {
	b, ok := value.([]byte)
	if !ok {
		return errors.Errorf("value is not []byte: %T", value)
	}

	return json.Unmarshal(b, &s)
}

func (s DockerStep) Value() (driver.Value, error) {
	return json.Marshal(s)
}

type DirtyRepository struct {
	RepositoryID   int
	RepositoryName string
	DirtyToken     int
}

type GetIndexersOptions struct {
	RepositoryID int
}

type GetUploadsOptions struct {
	RepositoryID            int
	State                   string
	States                  []string
	Term                    string
	VisibleAtTip            bool
	DependencyOf            int
	DependentOf             int
	IndexerNames            []string
	UploadedBefore          *time.Time
	UploadedAfter           *time.Time
	LastRetentionScanBefore *time.Time
	AllowExpired            bool
	AllowDeletedRepo        bool
	AllowDeletedUpload      bool
	OldestFirst             bool
	Limit                   int
	Offset                  int

	// InCommitGraph ensures that the repository commit graph was updated strictly
	// after this upload was processed. This condition helps us filter out new uploads
	// that we might later mistake for unreachable.
	InCommitGraph bool
}

type ReindexUploadsOptions struct {
	States       []string
	IndexerNames []string
	Term         string
	RepositoryID int
	VisibleAtTip bool
}

type DeleteUploadsOptions struct {
	RepositoryID int
	States       []string
	IndexerNames []string
	Term         string
	VisibleAtTip bool
}

// Package pairs a package scheme+manager+name+version with the dump that provides it.
type Package struct {
	DumpID  int
	Scheme  string
	Manager string
	Name    string
	Version string
}

// PackageReference is a package scheme+name+version
type PackageReference struct {
	Package
}

// PackageReferenceScanner allows for on-demand scanning of PackageReference values.
//
// A scanner for this type was introduced as a memory optimization. Instead of reading a
// large number of large byte arrays into memory at once, we allow the user to request
// the next filter value when they are ready to process it. This allows us to hold only
// a single bloom filter in memory at any give time during reference requests.
type PackageReferenceScanner interface {
	// Next reads the next package reference value from the database cursor.
	Next() (PackageReference, bool, error)

	// Close the underlying row object.
	Close() error
}

type GetIndexesOptions struct {
	RepositoryID  int
	State         string
	States        []string
	Term          string
	IndexerNames  []string
	WithoutUpload bool
	Limit         int
	Offset        int
}

type DeleteIndexesOptions struct {
	States        []string
	IndexerNames  []string
	Term          string
	RepositoryID  int
	WithoutUpload bool
}

type ReindexIndexesOptions struct {
	States        []string
	IndexerNames  []string
	Term          string
	RepositoryID  int
	WithoutUpload bool
}

type ExportedUpload struct {
	UploadID         int
	ExportedUploadID int
	Repo             string
	RepoID           int
	Root             string
}

type IndexesWithRepositoryNamespace struct {
	Root    string
	Indexer string
	Indexes []Index
}

type RepositoryWithCount struct {
	RepositoryID int
	Count        int
}

type RepositoryWithAvailableIndexers struct {
	RepositoryID      int
	AvailableIndexers map[string]AvailableIndexer
}

type UploadsWithRepositoryNamespace struct {
	Root    string
	Indexer string
	Uploads []Upload
}
