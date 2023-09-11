package apitest

import (
	"github.com/sourcegraph/go-diff/diff"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	btypes "github.com/sourcegraph/sourcegraph/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
	"github.com/sourcegraph/sourcegraph/lib/batches"
)

type GitTarget struct {
	OID            string
	AbbreviatedOID string
	TargetType     string `json:"type"`
}

type DiffRange struct{ StartLine, Lines int }

type DiffStat struct{ Added, Deleted int32 }

func (ds DiffStat) ToDiffStat() *diff.Stat {
	return &diff.Stat{Added: ds.Added, Deleted: ds.Deleted}
}

type FileDiffHunk struct {
	Body, Section      string
	OldNoNewlineAt     bool
	OldRange, NewRange DiffRange
}

type File struct {
	Name string
	// Ignoring other fields of File2, since that would require gitserver
}

type FileDiff struct {
	OldPath, NewPath string
	Hunks            []FileDiffHunk
	Stat             DiffStat
	OldFile          File
}

type FileDiffs struct {
	RawDiff    string
	DiffStat   DiffStat
	PageInfo   PageInfo
	Nodes      []FileDiff
	TotalCount int
}

type User struct {
	ID         string
	DatabaseID int32
	SiteAdmin  bool

	BatchChanges          BatchChangeConnection
	BatchChangesCodeHosts BatchChangesCodeHostsConnection
}

type Org struct {
	ID   string
	Name string

	BatchChanges BatchChangeConnection
}

type UserOrg struct {
	ID         string
	DatabaseID int32
	SiteAdmin  bool
	Name       string
}

type BatchChange struct {
	ID                      string
	Name                    string
	Description             string
	State                   btypes.BatchChangeState
	SpecCreator             *User
	Creator                 *User
	LastApplier             *User
	LastAppliedAt           string
	ViewerCanAdminister     bool
	Namespace               UserOrg
	CreatedAt               string
	UpdatedAt               string
	ClosedAt                string
	URL                     string
	ChangesetsStats         ChangesetsStats
	Changesets              ChangesetConnection
	ChangesetCountsOverTime []ChangesetCounts
	DiffStat                DiffStat
	BulkOperations          BulkOperationConnection
	BatchSpecs              BatchSpecConnection
}

type BatchChangeConnection struct {
	Nodes      []BatchChange
	TotalCount int
	PageInfo   PageInfo
}

type ChangesetEvent struct {
	ID        string
	Changeset struct{ ID string }
	CreatedAt string
}

type ChangesetEventConnection struct {
	TotalCount int
	PageInfo   PageInfo
	Nodes      []ChangesetEvent
}

type Repository struct {
	ID   string
	Name string
}

type ExternalURL struct {
	URL         string
	ServiceKind string
	ServiceType string
}

type GitHubCommitVerification struct {
	Verified  bool
	Reason    string
	Signature string
	Payload   string
}

type Changeset struct {
	Typename   string `json:"__typename"`
	ID         string
	Repository Repository

	BatchChanges       BatchChangeConnection
	OwnedByBatchChange *string

	CreatedAt          string
	UpdatedAt          string
	NextSyncAt         string
	ScheduleEstimateAt string
	Title              string
	Body               string
	Error              string
	State              string
	ExternalID         string
	ExternalURL        ExternalURL
	ForkNamespace      string
	ForkName           string
	ReviewState        string
	CheckState         string
	Events             ChangesetEventConnection

	CommitVerification *GitHubCommitVerification

	Diff Comparison

	Labels []Label

	CurrentSpec ChangesetSpec
}

type Comparison struct {
	Typename  string `json:"__typename"`
	FileDiffs FileDiffs
}

type Label struct {
	Text        string
	Color       string
	Description *string
}

type ChangesetConnection struct {
	Nodes      []Changeset
	TotalCount int
	PageInfo   PageInfo
}

type ChangesetsStats struct {
	Unpublished int
	Draft       int
	Open        int
	Merged      int
	Closed      int
	Deleted     int
	Total       int
}

type ChangesetCounts struct {
	Date                 string
	Total                int32
	Merged               int32
	Closed               int32
	Open                 int32
	Draft                int32
	OpenApproved         int32
	OpenChangesRequested int32
	OpenPending          int32
}

type BatchSpec struct {
	Typename string `json:"__typename"`
	ID       string

	OriginalInput string
	ParsedInput   graphqlbackend.JSONValue

	ApplyURL *string

	Namespace UserOrg
	Creator   *User

	ChangesetSpecs ChangesetSpecConnection
	ApplyPreview   ChangesetApplyPreviewConnection

	ViewerCanAdminister bool

	DiffStat DiffStat

	// Alias for the above.
	AllCodeHosts BatchChangesCodeHostsConnection
	// Alias for the above.
	OnlyWithoutCredential BatchChangesCodeHostsConnection

	CreatedAt gqlutil.DateTime
	ExpiresAt *gqlutil.DateTime

	// NEW
	SupersedingBatchSpec *BatchSpec
	AppliesToBatchChange BatchChange

	State               string
	WorkspaceResolution BatchSpecWorkspaceResolution

	StartedAt      gqlutil.DateTime
	FinishedAt     gqlutil.DateTime
	FailureMessage string
	ViewerCanRetry bool
}

type BatchSpecConnection struct {
	Nodes      []BatchSpec
	TotalCount int
	PageInfo   PageInfo
}

type BatchSpecWorkspaceResolution struct {
	State      string
	Workspaces BatchSpecWorkspaceConnection
}

type BatchSpecWorkspaceConnection struct {
	Nodes      []BatchSpecWorkspace
	TotalCount int
	PageInfo   PageInfo
}

// ChangesetSpecDelta is the delta between two ChangesetSpecs describing the same Changeset.
type ChangesetSpecDelta struct {
	TitleChanged         bool
	BodyChanged          bool
	Undraft              bool
	BaseRefChanged       bool
	DiffChanged          bool
	CommitMessageChanged bool
	AuthorNameChanged    bool
	AuthorEmailChanged   bool
}

type ChangesetSpec struct {
	Typename string `json:"__typename"`
	ID       string

	Description ChangesetSpecDescription

	ExpiresAt *gqlutil.DateTime
}

type ChangesetSpecConnection struct {
	Nodes      []ChangesetSpec
	TotalCount int
	PageInfo   PageInfo
}

type ChangesetApplyPreviewConnection struct {
	Nodes      []ChangesetApplyPreview
	TotalCount int
	PageInfo   PageInfo
	Stats      ChangesetApplyPreviewConnectionStats
}

type ChangesetApplyPreviewConnectionStats struct {
	Push         int32
	Update       int32
	Undraft      int32
	Publish      int32
	PublishDraft int32
	Sync         int32
	Import       int32
	Close        int32
	Reopen       int32
	Sleep        int32
	Detach       int32
}

type ChangesetApplyPreview struct {
	Typename string `json:"__typename"`

	Operations []btypes.ReconcilerOperation
	Delta      ChangesetSpecDelta
	Targets    ChangesetApplyPreviewTargets
}

type ChangesetApplyPreviewTargets struct {
	Typename string `json:"__typename"`

	ChangesetSpec ChangesetSpec
	Changeset     Changeset
}

type ChangesetSpecDescription struct {
	Typename string `json:"__typename"`

	BaseRepository Repository
	ExternalID     string
	BaseRef        string

	HeadRef string

	Title string
	Body  string

	Commits []GitCommitDescription

	Published batches.PublishedValue

	Diff struct {
		FileDiffs FileDiffs
	}
	DiffStat DiffStat
}

type GitCommitDescription struct {
	Author  Person
	Message string
	Subject string
	Body    string
	Diff    string
}

type PageInfo struct {
	HasNextPage bool
	EndCursor   *string
}

type Person struct {
	Name  string
	Email string
	User  *User
}

type BatchChangesCredential struct {
	ID                  string
	ExternalServiceKind string
	ExternalServiceURL  string
	IsSiteCredential    bool
	CreatedAt           string
}

type EmptyResponse struct {
	AlwaysNil string
}

type BatchChangesCodeHostsConnection struct {
	PageInfo   PageInfo
	Nodes      []BatchChangesCodeHost
	TotalCount int
}

type BatchChangesCodeHost struct {
	ExternalServiceKind string
	ExternalServiceURL  string
	Credential          BatchChangesCredential
}

type BulkOperation struct {
	ID         string
	Type       string
	State      string
	Progress   float64
	Errors     []*ChangesetJobError
	CreatedAt  string
	FinishedAt string
}

type ChangesetJobError struct {
	Changeset *Changeset
	Error     *string
}

type BulkOperationConnection struct {
	Nodes      []BulkOperation
	TotalCount int
	PageInfo   PageInfo
}

type GitRef struct {
	Name        string
	DisplayName string
	AbbrevName  string
	Target      GitTarget
}

type BatchSpecWorkspace struct {
	Typename string `json:"__typename"`
	ID       string

	Repository Repository
	BatchSpec  BatchSpec

	ChangesetSpecs []ChangesetSpec

	Branch            GitRef
	Path              string
	SearchResultPaths []string
	Steps             []BatchSpecWorkspaceStep

	CachedResultFound  bool
	OnlyFetchWorkspace bool
	Ignored            bool
	Unsupported        bool

	State          string
	StartedAt      gqlutil.DateTime
	FinishedAt     gqlutil.DateTime
	FailureMessage string
	PlaceInQueue   int
}

type BatchSpecWorkspaceStep struct {
	Run       string
	Container string
}
