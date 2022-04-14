package bitbucketcloud

import (
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Types that are returned by Bitbucket Cloud calls.

type Account struct {
	Links         Links         `json:"links"`
	Username      string        `json:"username"`
	Nickname      string        `json:"nickname"`
	AccountStatus AccountStatus `json:"account_status"`
	DisplayName   string        `json:"display_name"`
	Website       string        `json:"website"`
	CreatedOn     time.Time     `json:"created_on"`
	UUID          string        `json:"uuid"`
}

type Comment struct {
	ID        int64          `json:"id"`
	CreatedOn time.Time      `json:"created_on"`
	UpdatedOn time.Time      `json:"updated_on"`
	Content   RenderedMarkup `json:"content"`
	User      User           `json:"user"`
	Deleted   bool           `json:"deleted"`
	Parent    *Comment       `json:"parent,omitempty"`
	Inline    *CommentInline `json:"inline,omitempty"`
	Links     Links          `json:"links"`
}

type CommentInline struct {
	To   int64  `json:"to,omitempty"`
	From int64  `json:"from,omitempty"`
	Path string `json:"path"`
}

type Link struct {
	Href string `json:"href"`
	Name string `json:"name,omitempty"`
}

type Links map[string]Link

type Participant struct {
	User           User             `json:"user"`
	Role           ParticipantRole  `json:"role"`
	Approved       bool             `json:"approved"`
	State          ParticipantState `json:"state"`
	ParticipatedOn time.Time        `json:"participated_on"`
}

// PullRequest represents a single pull request, as returned by the API.
type PullRequest struct {
	Links             Links                     `json:"links"`
	ID                int64                     `json:"id"`
	Title             string                    `json:"title"`
	Rendered          RenderedPullRequestMarkup `json:"rendered"`
	Summary           RenderedMarkup            `json:"summary"`
	State             PullRequestState          `json:"state"`
	Author            Account                   `json:"author"`
	Source            PullRequestEndpoint       `json:"source"`
	Destination       PullRequestEndpoint       `json:"destination"`
	MergeCommit       *PullRequestCommit        `json:"merge_commit,omitempty"`
	CommentCount      int64                     `json:"comment_count"`
	TaskCount         int64                     `json:"task_count"`
	CloseSourceBranch bool                      `json:"close_source_branch"`
	ClosedBy          *Account                  `json:"account,omitempty"`
	Reason            *string                   `json:"reason,omitempty"`
	CreatedOn         time.Time                 `json:"created_on"`
	UpdatedOn         time.Time                 `json:"updated_on"`
	Reviewers         []Account                 `json:"reviewers"`
	Participants      []Participant             `json:"participants"`
}

type PullRequestBranch struct {
	Name                 string          `json:"name"`
	MergeStrategies      []MergeStrategy `json:"merge_strategies"`
	DefaultMergeStrategy MergeStrategy   `json:"default_merge_strategy"`
}

type PullRequestCommit struct {
	Hash string `json:"hash"`
}

type PullRequestEndpoint struct {
	Repo   Repo              `json:"repository"`
	Branch PullRequestBranch `json:"branch"`
	Commit PullRequestCommit `json:"commit"`
}

type RenderedPullRequestMarkup struct {
	Title       RenderedMarkup `json:"title"`
	Description RenderedMarkup `json:"description"`
	Reason      RenderedMarkup `json:"reason"`
}

type PullRequestStatus struct {
	Links       Links                  `json:"links"`
	UUID        string                 `json:"uuid"`
	Key         string                 `json:"key"`
	RefName     string                 `json:"refname"`
	URL         string                 `json:"url"`
	State       PullRequestStatusState `json:"state"`
	Name        string                 `json:"name"`
	Description string                 `json:"description"`
	CreatedOn   time.Time              `json:"created_on"`
	UpdatedOn   time.Time              `json:"updated_on"`
}

type MergeStrategy string
type PullRequestState string
type PullRequestStatusState string

const (
	MergeStrategyMergeCommit MergeStrategy = "merge_commit"
	MergeStrategySquash      MergeStrategy = "squash"
	MergeStrategyFastForward MergeStrategy = "fast_forward"

	PullRequestStateMerged     PullRequestState = "MERGED"
	PullRequestStateSuperseded PullRequestState = "SUPERSEDED"
	PullRequestStateOpen       PullRequestState = "OPEN"
	PullRequestStateDeclined   PullRequestState = "DECLINED"

	PullRequestStatusStateSuccessful PullRequestStatusState = "SUCCESSFUL"
	PullRequestStatusStateFailed     PullRequestStatusState = "FAILED"
	PullRequestStatusStateInProgress PullRequestStatusState = "INPROGRESS"
	PullRequestStatusStateStopped    PullRequestStatusState = "STOPPED"
)

type RenderedMarkup struct {
	Raw    string `json:"raw"`
	Markup string `json:"markup"`
	HTML   string `json:"html"`
}

type AccountStatus string
type ParticipantRole string
type ParticipantState string

const (
	AccountStatusActive AccountStatus = "active"

	ParticipantRoleParticipant ParticipantRole = "PARTICIPANT"
	ParticipantRoleReviewer    ParticipantRole = "REVIEWER"

	ParticipantStateApproved         ParticipantState = "approved"
	ParticipantStateChangesRequested ParticipantState = "changes_requested"
	ParticipantStateNull             ParticipantState = "null"
)

// Repo represents the Repository type returned by Bitbucket Cloud.
//
// When used as an input into functions, only the FullName field is actually
// read.
type Repo struct {
	Slug        string     `json:"slug"`
	Name        string     `json:"name"`
	FullName    string     `json:"full_name"`
	UUID        string     `json:"uuid"`
	SCM         string     `json:"scm"`
	Description string     `json:"description"`
	Parent      *Repo      `json:"parent"`
	IsPrivate   bool       `json:"is_private"`
	Links       RepoLinks  `json:"links"`
	ForkPolicy  ForkPolicy `json:"fork_policy"`
}

func (r *Repo) Namespace() (string, error) {
	// Bitbucket Cloud will return cut down versions of the repository in some
	// cases (for example, embedded in pull requests), but we always have the
	// full name, so let's parse the namespace out of that.

	// TODO: replace with strings.Cut() once we upgrade to Go 1.18.
	parts := strings.SplitN(r.FullName, "/", 2)
	if len(parts) < 2 {
		return "", errors.New("cannot split namespace from repo name")
	}

	return parts[0], nil
}

type ForkPolicy string

const (
	ForkPolicyAllow    ForkPolicy = "allow_forks"
	ForkPolicyNoPublic ForkPolicy = "no_public_forks"
	ForkPolicyNone     ForkPolicy = "no_forks"
)

type RepoLinks struct {
	Clone CloneLinks `json:"clone"`
	HTML  Link       `json:"html"`
}

type CloneLinks []Link

// HTTPS returns clone link named "https", it returns an error if not found.
func (cl CloneLinks) HTTPS() (string, error) {
	for _, l := range cl {
		if l.Name == "https" {
			return l.Href, nil
		}
	}
	return "", errors.New("HTTPS clone link not found")
}
