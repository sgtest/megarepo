package protocol

import (
	"encoding/json"
	"time"

	"github.com/opentracing/opentracing-go/log"
	"go.opentelemetry.io/otel/attribute"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type SearchRequest struct {
	Repo                 api.RepoName
	Revisions            []RevisionSpecifier
	Query                Node
	IncludeDiff          bool
	Limit                int
	IncludeModifiedFiles bool
}

func (r *SearchRequest) ToProto() *proto.SearchRequest {
	revs := make([]*proto.RevisionSpecifier, 0, len(r.Revisions))
	for _, rev := range r.Revisions {
		revs = append(revs, rev.ToProto())
	}
	return &proto.SearchRequest{
		Repo:                 string(r.Repo),
		Revisions:            revs,
		Query:                r.Query.ToProto(),
		IncludeDiff:          r.IncludeDiff,
		Limit:                int64(r.Limit),
		IncludeModifiedFiles: r.IncludeModifiedFiles,
	}
}

func SearchRequestFromProto(p *proto.SearchRequest) (*SearchRequest, error) {
	query, err := NodeFromProto(p.GetQuery())
	if err != nil {
		return nil, err
	}

	revisions := make([]RevisionSpecifier, 0, len(p.GetRevisions()))
	for _, rev := range p.GetRevisions() {
		revisions = append(revisions, RevisionSpecifierFromProto(rev))
	}

	return &SearchRequest{
		Repo:                 api.RepoName(p.GetRepo()),
		Revisions:            revisions,
		Query:                query,
		IncludeDiff:          p.GetIncludeDiff(),
		Limit:                int(p.GetLimit()),
		IncludeModifiedFiles: p.GetIncludeModifiedFiles(),
	}, nil
}

type RevisionSpecifier struct {
	// RevSpec is a revision range specifier suitable for passing to git. See
	// the manpage gitrevisions(7).
	RevSpec string

	// RefGlob is a reference glob to pass to git. See the documentation for
	// "--glob" in git-log.
	RefGlob string

	// ExcludeRefGlob is a glob for references to exclude. See the
	// documentation for "--exclude" in git-log.
	ExcludeRefGlob string
}

func (r *RevisionSpecifier) ToProto() *proto.RevisionSpecifier {
	return &proto.RevisionSpecifier{
		RevSpec:        r.RevSpec,
		RefGlob:        r.RefGlob,
		ExcludeRefGlob: r.ExcludeRefGlob,
	}
}

func RevisionSpecifierFromProto(p *proto.RevisionSpecifier) RevisionSpecifier {
	return RevisionSpecifier{
		RevSpec:        p.GetRevSpec(),
		RefGlob:        p.GetRefGlob(),
		ExcludeRefGlob: p.GetExcludeRefGlob(),
	}
}

type SearchEventMatches []CommitMatch

type SearchEventDone struct {
	LimitHit bool
	Error    string
}

func (s SearchEventDone) Err() error {
	if s.Error != "" {
		var e gitdomain.RepoNotExistError
		if err := json.Unmarshal([]byte(s.Error), &e); err != nil {
			return &e
		}
		return errors.New(s.Error)
	}
	return nil
}

func NewSearchEventDone(limitHit bool, err error) SearchEventDone {
	event := SearchEventDone{
		LimitHit: limitHit,
	}
	var notExistError *gitdomain.RepoNotExistError
	if errors.As(err, &notExistError) {
		b, _ := json.Marshal(notExistError)
		event.Error = string(b)
	} else if err != nil {
		event.Error = err.Error()
	}
	return event
}

type CommitMatch struct {
	Oid        api.CommitID
	Author     Signature      `json:",omitempty"`
	Committer  Signature      `json:",omitempty"`
	Parents    []api.CommitID `json:",omitempty"`
	Refs       []string       `json:",omitempty"`
	SourceRefs []string       `json:",omitempty"`

	Message       result.MatchedString `json:",omitempty"`
	Diff          result.MatchedString `json:",omitempty"`
	ModifiedFiles []string             `json:",omitempty"`
}

func (cm *CommitMatch) ToProto() *proto.CommitMatch {
	parents := make([]string, 0, len(cm.Parents))
	for _, parent := range cm.Parents {
		parents = append(parents, string(parent))
	}
	return &proto.CommitMatch{
		Oid:           string(cm.Oid),
		Author:        cm.Author.ToProto(),
		Committer:     cm.Committer.ToProto(),
		Parents:       parents,
		Refs:          cm.Refs,
		SourceRefs:    cm.SourceRefs,
		Message:       matchedStringToProto(cm.Message),
		Diff:          matchedStringToProto(cm.Diff),
		ModifiedFiles: cm.ModifiedFiles,
	}
}

func CommitMatchFromProto(p *proto.CommitMatch) CommitMatch {
	parents := make([]api.CommitID, 0, len(p.GetParents()))
	for _, parent := range p.GetParents() {
		parents = append(parents, api.CommitID(parent))
	}
	return CommitMatch{
		Oid:           api.CommitID(p.GetOid()),
		Author:        SignatureFromProto(p.GetAuthor()),
		Committer:     SignatureFromProto(p.GetCommitter()),
		Parents:       parents,
		Refs:          p.GetRefs(),
		SourceRefs:    p.GetSourceRefs(),
		Message:       matchedStringFromProto(p.GetMessage()),
		Diff:          matchedStringFromProto(p.GetDiff()),
		ModifiedFiles: p.GetModifiedFiles(),
	}
}

func matchedStringFromProto(p *proto.CommitMatch_MatchedString) result.MatchedString {
	ranges := make([]result.Range, 0, len(p.GetRanges()))
	for _, rr := range p.GetRanges() {
		ranges = append(ranges, rangeFromProto(rr))
	}
	return result.MatchedString{
		Content:       p.GetContent(),
		MatchedRanges: ranges,
	}
}

func matchedStringToProto(ms result.MatchedString) *proto.CommitMatch_MatchedString {
	rrs := make([]*proto.CommitMatch_Range, 0, len(ms.MatchedRanges))
	for _, rr := range ms.MatchedRanges {
		rrs = append(rrs, rangeToProto(rr))
	}
	return &proto.CommitMatch_MatchedString{
		Content: ms.Content,
		Ranges:  rrs,
	}
}

func rangeToProto(r result.Range) *proto.CommitMatch_Range {
	return &proto.CommitMatch_Range{
		Start: locationToProto(r.Start),
		End:   locationToProto(r.End),
	}
}

func rangeFromProto(p *proto.CommitMatch_Range) result.Range {
	return result.Range{
		Start: locationFromProto(p.GetStart()),
		End:   locationFromProto(p.GetEnd()),
	}
}

func locationToProto(l result.Location) *proto.CommitMatch_Location {
	return &proto.CommitMatch_Location{
		Offset: uint32(l.Offset),
		Line:   uint32(l.Line),
		Column: uint32(l.Column),
	}
}

func locationFromProto(p *proto.CommitMatch_Location) result.Location {
	return result.Location{
		Offset: int(p.GetOffset()),
		Line:   int(p.GetLine()),
		Column: int(p.GetColumn()),
	}
}

type Signature struct {
	Name  string `json:",omitempty"`
	Email string `json:",omitempty"`
	Date  time.Time
}

func (s *Signature) ToProto() *proto.CommitMatch_Signature {
	return &proto.CommitMatch_Signature{
		Name:  s.Name,
		Email: s.Email,
		Date:  timestamppb.New(s.Date),
	}
}

func SignatureFromProto(p *proto.CommitMatch_Signature) Signature {
	return Signature{
		Name:  p.GetName(),
		Email: p.GetEmail(),
		Date:  p.GetDate().AsTime(),
	}
}

// ExecRequest is a request to execute a command inside a git repository.
//
// Note that this request is deserialized by both gitserver and the frontend's
// internal proxy route and any major change to this structure will need to
// be reconciled in both places.
type ExecRequest struct {
	Repo api.RepoName `json:"repo"`

	EnsureRevision string   `json:"ensureRevision"`
	Args           []string `json:"args"`
	Stdin          []byte   `json:"stdin,omitempty"`
	NoTimeout      bool     `json:"noTimeout"`
}

// ListFilesOpts specifies options when calling gitserverClient.ListFiles.
type ListFilesOpts struct {
	IncludeDirs      bool
	MaxFileSizeBytes int64
}

// BatchLogRequest is a request to execute a `git log` command inside a set of
// git repositories present on the target shard.
type BatchLogRequest struct {
	RepoCommits []api.RepoCommit `json:"repoCommits"`

	// Format is the entire `--format=<format>` argument to git log. This value
	// is expected to be non-empty.
	Format string `json:"format"`
}

func (bl *BatchLogRequest) ToProto() *proto.BatchLogRequest {
	repoCommits := make([]*proto.RepoCommit, 0, len(bl.RepoCommits))
	for _, rc := range bl.RepoCommits {
		repoCommits = append(repoCommits, rc.ToProto())
	}
	return &proto.BatchLogRequest{
		RepoCommits: repoCommits,
		Format:      bl.Format,
	}
}

func (bl *BatchLogRequest) FromProto(p *proto.BatchLogRequest) {
	repoCommits := make([]api.RepoCommit, 0, len(p.GetRepoCommits()))
	for _, protoRc := range p.GetRepoCommits() {
		var rc api.RepoCommit
		rc.FromProto(protoRc)
		repoCommits = append(repoCommits, rc)
	}
	bl.RepoCommits = repoCommits
	bl.Format = p.GetFormat()
}

func (req BatchLogRequest) LogFields() []log.Field {
	return []log.Field{
		log.Int("numRepoCommits", len(req.RepoCommits)),
		log.String("format", req.Format),
	}
}

func (req BatchLogRequest) SpanAttributes() []attribute.KeyValue {
	return []attribute.KeyValue{
		attribute.Int("numRepoCommits", len(req.RepoCommits)),
		attribute.String("format", req.Format),
	}
}

type BatchLogResponse struct {
	Results []BatchLogResult `json:"results"`
}

func (bl *BatchLogResponse) ToProto() *proto.BatchLogResponse {
	results := make([]*proto.BatchLogResult, 0, len(bl.Results))
	for _, r := range bl.Results {
		results = append(results, r.ToProto())
	}
	return &proto.BatchLogResponse{
		Results: results,
	}
}

func (bl *BatchLogResponse) FromProto(p *proto.BatchLogResponse) {
	results := make([]BatchLogResult, 0, len(p.GetResults()))
	for _, protoR := range p.GetResults() {
		var r BatchLogResult
		r.FromProto(protoR)
		results = append(results, r)
	}
	*bl = BatchLogResponse{
		Results: results,
	}
}

// BatchLogResult associates a repository and commit pair from the input of a BatchLog
// request with the result of the associated git log command.
type BatchLogResult struct {
	RepoCommit    api.RepoCommit `json:"repoCommit"`
	CommandOutput string         `json:"output"`
	CommandError  string         `json:"error,omitempty"`
}

func (bl *BatchLogResult) ToProto() *proto.BatchLogResult {
	return &proto.BatchLogResult{
		RepoCommit:    bl.RepoCommit.ToProto(),
		CommandOutput: bl.CommandOutput,
		CommandError:  &bl.CommandError,
	}
}

func (bl *BatchLogResult) FromProto(p *proto.BatchLogResult) {
	var rc api.RepoCommit
	rc.FromProto(p.GetRepoCommit())

	*bl = BatchLogResult{
		RepoCommit:    rc,
		CommandOutput: p.GetCommandOutput(),
		CommandError:  p.GetCommandError(),
	}
}

// P4ExecRequest is a request to execute a p4 command with given arguments.
//
// Note that this request is deserialized by both gitserver and the frontend's
// internal proxy route and any major change to this structure will need to be
// reconciled in both places.
type P4ExecRequest struct {
	P4Port   string   `json:"p4port"`
	P4User   string   `json:"p4user"`
	P4Passwd string   `json:"p4passwd"`
	Args     []string `json:"args"`
}

func (r *P4ExecRequest) ToProto() *proto.P4ExecRequest {
	return &proto.P4ExecRequest{
		P4Port:   r.P4Port,
		P4User:   r.P4User,
		P4Passwd: r.P4Passwd,
		Args:     r.Args,
	}
}

func (r *P4ExecRequest) FromProto(p *proto.P4ExecRequest) {
	*r = P4ExecRequest{
		P4Port:   p.GetP4Port(),
		P4User:   p.GetP4User(),
		P4Passwd: p.GetP4Passwd(),
		Args:     p.GetArgs(),
	}
}

// RepoUpdateRequest is a request to update the contents of a given repo, or clone it if it doesn't exist.
type RepoUpdateRequest struct {
	Repo  api.RepoName  `json:"repo"`  // identifying URL for repo
	Since time.Duration `json:"since"` // debounce interval for queries, used only with request-repo-update

	// CloneFromShard is the hostname of the gitserver instance that is the current owner of the
	// repository. If this is set, then the RepoUpdateRequest is to migrate the repo from
	// that gitserver instance to the new home of the repo.
	CloneFromShard string `json:"cloneFromShard"`
}

func (r *RepoUpdateRequest) ToProto() *proto.RepoUpdateRequest {
	return &proto.RepoUpdateRequest{
		Repo:           string(r.Repo),
		Since:          durationpb.New(r.Since),
		CloneFromShard: r.CloneFromShard,
	}
}

func (r *RepoUpdateRequest) FromProto(p *proto.RepoUpdateRequest) {
	*r = RepoUpdateRequest{
		Repo:           api.RepoName(p.GetRepo()),
		Since:          p.GetSince().AsDuration(),
		CloneFromShard: p.GetCloneFromShard(),
	}
}

// RepoUpdateResponse returns meta information of the repo enqueued for update.
type RepoUpdateResponse struct {
	LastFetched *time.Time `json:",omitempty"`
	LastChanged *time.Time `json:",omitempty"`

	// Error is an error reported by the update operation, and not a network protocol error.
	Error string `json:",omitempty"`
}

func (r *RepoUpdateResponse) ToProto() *proto.RepoUpdateResponse {
	var lastFetched, lastChanged *timestamppb.Timestamp
	if r.LastFetched != nil {
		lastFetched = timestamppb.New(*r.LastFetched)
	}

	if r.LastChanged != nil {
		lastChanged = timestamppb.New(*r.LastChanged)
	}

	return &proto.RepoUpdateResponse{
		LastFetched: timestamppb.New(lastFetched.AsTime()),
		LastChanged: timestamppb.New(lastChanged.AsTime()),
		Error:       r.Error,
	}
}

func (r *RepoUpdateResponse) FromProto(p *proto.RepoUpdateResponse) {
	var lastFetched, lastChanged time.Time
	if p.GetLastFetched() != nil {
		lf := p.GetLastFetched().AsTime()
		lastFetched = lf
	} else {
		lastFetched = time.Time{}
	}

	if p.GetLastChanged() != nil {
		lc := p.GetLastChanged().AsTime()
		lastChanged = lc
	} else {
		lastChanged = time.Time{}
	}

	*r = RepoUpdateResponse{
		LastFetched: &lastFetched,
		LastChanged: &lastChanged,
		Error:       p.GetError(),
	}
}

// RepoCloneRequest is a request to clone a repository asynchronously.
type RepoCloneRequest struct {
	Repo api.RepoName `json:"repo"`
}

// RepoCloneResponse returns an error if the repo clone request failed.
type RepoCloneResponse struct {
	Error string `json:",omitempty"`
}

func (r *RepoCloneResponse) ToProto() *proto.RepoCloneResponse {
	return &proto.RepoCloneResponse{
		Error: r.Error,
	}
}

func (r *RepoCloneResponse) FromProto(p *proto.RepoCloneResponse) {
	*r = RepoCloneResponse{
		Error: p.GetError(),
	}
}

type NotFoundPayload struct {
	CloneInProgress bool `json:"cloneInProgress"` // If true, exec returned with noop because clone is in progress.

	// CloneProgress is a progress message from the running clone command.
	CloneProgress string `json:"cloneProgress,omitempty"`
}

// IsRepoCloneableRequest is a request to determine if a repo is cloneable.
type IsRepoCloneableRequest struct {
	// Repo is the repository to check.
	Repo api.RepoName `json:"Repo"`
}

// IsRepoCloneableResponse is the response type for the IsRepoCloneableRequest.
type IsRepoCloneableResponse struct {
	Cloneable bool   // whether the repo is cloneable
	Cloned    bool   // true if the repo was ever cloned in the past
	Reason    string // if not cloneable, the reason why not
}

func (i *IsRepoCloneableResponse) ToProto() *proto.IsRepoCloneableResponse {
	return &proto.IsRepoCloneableResponse{
		Cloneable: i.Cloneable,
		Cloned:    i.Cloned,
		Reason:    i.Reason,
	}
}

func (i *IsRepoCloneableResponse) FromProto(p *proto.IsRepoCloneableResponse) {
	*i = IsRepoCloneableResponse{
		Cloneable: p.GetCloneable(),
		Cloned:    p.GetCloned(),
		Reason:    p.GetReason(),
	}
}

// RepoDeleteRequest is a request to delete a repository clone on gitserver
type RepoDeleteRequest struct {
	// Repo is the repository to delete.
	Repo api.RepoName
}

// ReposStats is an aggregation of statistics from a gitserver.
type ReposStats struct {
	// UpdatedAt is the time these statistics were computed. If UpdateAt is
	// zero, the statistics have not yet been computed. This can happen on a
	// new gitserver.
	UpdatedAt time.Time

	// GitDirBytes is the amount of bytes stored in .git directories.
	GitDirBytes int64
}

func (rs *ReposStats) FromProto(x *proto.ReposStatsResponse) {
	protoGitDirBytes := x.GetGitDirBytes()
	protoUpdatedAt := x.GetUpdatedAt().AsTime()

	*rs = ReposStats{
		UpdatedAt:   protoUpdatedAt,
		GitDirBytes: int64(protoGitDirBytes),
	}
}

func (rs *ReposStats) ToProto() *proto.ReposStatsResponse {
	return &proto.ReposStatsResponse{
		GitDirBytes: uint64(rs.GitDirBytes),
		UpdatedAt:   timestamppb.New(rs.UpdatedAt),
	}
}

// RepoCloneProgressRequest is a request for information about the clone progress of multiple
// repositories on gitserver.
type RepoCloneProgressRequest struct {
	Repos []api.RepoName
}

// RepoCloneProgress is information about the clone progress of a repo
type RepoCloneProgress struct {
	CloneInProgress bool   // whether the repository is currently being cloned
	CloneProgress   string // a progress message from the running clone command.
	Cloned          bool   // whether the repository has been cloned successfully
}

func (r *RepoCloneProgress) FromProto(p *proto.RepoCloneProgress) {
	*r = RepoCloneProgress{
		CloneInProgress: p.GetCloneInProgress(),
		CloneProgress:   p.GetCloneProgress(),
		Cloned:          p.GetCloned(),
	}
}

// RepoCloneProgressResponse is the response to a repository clone progress request
// for multiple repositories at the same time.
type RepoCloneProgressResponse struct {
	Results map[api.RepoName]*RepoCloneProgress
}

func (r *RepoCloneProgressResponse) ToProto() *proto.RepoCloneProgressResponse {
	results := make(map[string]*proto.RepoCloneProgress, len(r.Results))
	for k, v := range r.Results {
		results[string(k)] = &proto.RepoCloneProgress{
			CloneInProgress: v.CloneInProgress,
			CloneProgress:   v.CloneProgress,
			Cloned:          v.Cloned,
		}
	}
	return &proto.RepoCloneProgressResponse{
		Results: results,
	}
}

func (r *RepoCloneProgressResponse) FromProto(p *proto.RepoCloneProgressResponse) {
	results := make(map[api.RepoName]*RepoCloneProgress, len(p.GetResults()))
	for k, v := range p.GetResults() {
		results[api.RepoName(k)] = &RepoCloneProgress{
			CloneInProgress: v.GetCloneInProgress(),
			CloneProgress:   v.GetCloneProgress(),
			Cloned:          v.GetCloned(),
		}
	}
	*r = RepoCloneProgressResponse{
		Results: results,
	}
}

// CreateCommitFromPatchRequest is the request information needed for creating
// the simulated staging area git object for a repo.
type CreateCommitFromPatchRequest struct {
	// Repo is the repository to get information about.
	Repo api.RepoName
	// BaseCommit is the revision that the staging area object is based on
	BaseCommit api.CommitID
	// Patch is the diff contents to be used to create the staging area revision
	Patch []byte
	// TargetRef is the ref that will be created for this patch
	TargetRef string
	// If set to true and the TargetRef already exists, an unique number will be appended to the end (ie TargetRef-{#}). The generated ref will be returned.
	UniqueRef bool
	// CommitInfo is the information that will be used when creating the commit from a patch
	CommitInfo PatchCommitInfo
	// Push specifies whether the target ref will be pushed to the code host: if
	// nil, no push will be attempted, if non-nil, a push will be attempted.
	Push *PushConfig
	// GitApplyArgs are the arguments that will be passed to `git apply` along
	// with `--cached`.
	GitApplyArgs []string
}

func (c *CreateCommitFromPatchRequest) ToProto() *proto.CreateCommitFromPatchBinaryRequest {
	return &proto.CreateCommitFromPatchBinaryRequest{
		Repo:         string(c.Repo),
		BaseCommit:   string(c.BaseCommit),
		Patch:        c.Patch,
		TargetRef:    c.TargetRef,
		UniqueRef:    c.UniqueRef,
		CommitInfo:   c.CommitInfo.ToProto(),
		Push:         c.Push.ToProto(),
		GitApplyArgs: c.GitApplyArgs,
	}
}

func (c *CreateCommitFromPatchRequest) FromProto(p *proto.CreateCommitFromPatchBinaryRequest) {
	*c = CreateCommitFromPatchRequest{
		Repo:         api.RepoName(p.GetRepo()),
		BaseCommit:   api.CommitID(p.GetBaseCommit()),
		Patch:        p.GetPatch(),
		TargetRef:    p.GetTargetRef(),
		UniqueRef:    p.GetUniqueRef(),
		CommitInfo:   PatchCommitInfoFromProto(p.GetCommitInfo()),
		Push:         PushConfigFromProto(p.GetPush()),
		GitApplyArgs: p.GetGitApplyArgs(),
	}
}

// PatchCommitInfo will be used for commit information when creating a commit from a patch
type PatchCommitInfo struct {
	Message        string
	AuthorName     string
	AuthorEmail    string
	CommitterName  string
	CommitterEmail string
	Date           time.Time
}

func (p *PatchCommitInfo) ToProto() *proto.PatchCommitInfo {
	return &proto.PatchCommitInfo{
		Message:        p.Message,
		AuthorName:     p.AuthorName,
		AuthorEmail:    p.AuthorEmail,
		CommitterName:  p.CommitterName,
		CommitterEmail: p.CommitterEmail,
		Date:           timestamppb.New(p.Date),
	}
}

func PatchCommitInfoFromProto(p *proto.PatchCommitInfo) PatchCommitInfo {
	return PatchCommitInfo{
		Message:        p.GetMessage(),
		AuthorName:     p.GetAuthorName(),
		AuthorEmail:    p.GetAuthorEmail(),
		CommitterName:  p.GetCommitterName(),
		CommitterEmail: p.GetCommitterEmail(),
		Date:           p.GetDate().AsTime(),
	}
}

// PushConfig provides the configuration required to push one or more commits to
// a code host.
type PushConfig struct {
	// RemoteURL is the git remote URL to which to push the commits.
	// The URL needs to include HTTP basic auth credentials if no
	// unauthenticated requests are allowed by the remote host.
	RemoteURL string

	// PrivateKey is used when the remote URL uses scheme `ssh`. If set,
	// this value is used as the content of the private key. Needs to be
	// set in conjunction with a passphrase.
	PrivateKey string

	// Passphrase is the passphrase to decrypt the private key. It is required
	// when passing PrivateKey.
	Passphrase string
}

func (p *PushConfig) ToProto() *proto.PushConfig {
	return &proto.PushConfig{
		RemoteUrl:  p.RemoteURL,
		PrivateKey: p.PrivateKey,
		Passphrase: p.Passphrase,
	}
}

func PushConfigFromProto(p *proto.PushConfig) *PushConfig {
	if p == nil {
		return nil
	}
	return &PushConfig{
		RemoteURL:  p.GetRemoteUrl(),
		PrivateKey: p.GetPrivateKey(),
		Passphrase: p.GetPassphrase(),
	}
}

// CreateCommitFromPatchResponse is the response type returned after creating
// a commit from a patch
type CreateCommitFromPatchResponse struct {
	// Rev is the tag that the staging object can be found at
	Rev string

	// Error is populated only on error
	Error *CreateCommitFromPatchError
}

func (r *CreateCommitFromPatchResponse) ToProto() *proto.CreateCommitFromPatchBinaryResponse {
	var err *proto.CreateCommitFromPatchError
	if r.Error != nil {
		err = r.Error.ToProto()
	} else {
		err = nil
	}
	return &proto.CreateCommitFromPatchBinaryResponse{
		Rev:   r.Rev,
		Error: err,
	}
}

func (r *CreateCommitFromPatchResponse) FromProto(p *proto.CreateCommitFromPatchBinaryResponse) {
	if p.GetError() == nil {
		r.Error = nil
	} else {
		r.Error = &CreateCommitFromPatchError{}
		r.Error.FromProto(p.GetError())
	}
	r.Rev = p.GetRev()
}

// SetError adds the supplied error related details to e.
func (e *CreateCommitFromPatchResponse) SetError(repo, command, out string, err error) {
	if e.Error == nil {
		e.Error = &CreateCommitFromPatchError{}
	}
	e.Error.RepositoryName = repo
	e.Error.Command = command
	e.Error.CombinedOutput = out
	e.Error.InternalError = err.Error()
}

// CreateCommitFromPatchError is populated on errors running
// CreateCommitFromPatch
type CreateCommitFromPatchError struct {
	// RepositoryName is the name of the repository
	RepositoryName string

	// InternalError is the internal error
	InternalError string

	// Command is the last git command that was attempted
	Command string
	// CombinedOutput is the combined stderr and stdout from running the command
	CombinedOutput string
}

func (e *CreateCommitFromPatchError) ToProto() *proto.CreateCommitFromPatchError {
	return &proto.CreateCommitFromPatchError{
		RepositoryName: e.RepositoryName,
		InternalError:  e.InternalError,
		Command:        e.Command,
		CombinedOutput: e.CombinedOutput,
	}
}

func (e *CreateCommitFromPatchError) FromProto(p *proto.CreateCommitFromPatchError) {
	*e = CreateCommitFromPatchError{
		RepositoryName: p.GetRepositoryName(),
		InternalError:  p.GetInternalError(),
		Command:        p.GetCommand(),
		CombinedOutput: p.GetCombinedOutput(),
	}
}

// Error returns a detailed error conforming to the error interface
func (e *CreateCommitFromPatchError) Error() string {
	return e.InternalError
}

type GetObjectRequest struct {
	Repo       api.RepoName
	ObjectName string
}

func (r *GetObjectRequest) ToProto() *proto.GetObjectRequest {
	return &proto.GetObjectRequest{
		Repo:       string(r.Repo),
		ObjectName: r.ObjectName,
	}
}

func (r *GetObjectRequest) FromProto(p *proto.GetObjectRequest) {
	*r = GetObjectRequest{
		Repo:       api.RepoName(p.GetRepo()),
		ObjectName: p.GetObjectName(),
	}
}

type GetObjectResponse struct {
	Object gitdomain.GitObject
}

func (r *GetObjectResponse) ToProto() *proto.GetObjectResponse {
	return &proto.GetObjectResponse{
		Object: r.Object.ToProto(),
	}
}

func (r *GetObjectResponse) FromProto(p *proto.GetObjectResponse) {
	obj := p.GetObject()

	var gitObj gitdomain.GitObject
	gitObj.FromProto(obj)
	*r = GetObjectResponse{
		Object: gitObj,
	}

}
