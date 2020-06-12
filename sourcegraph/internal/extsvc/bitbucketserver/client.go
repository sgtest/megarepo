package bitbucketserver

import (
	"bytes"
	"context"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/RoaringBitmap/roaring"
	"github.com/gomodule/oauth1/oauth"
	"github.com/inconshreveable/log15"
	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/pkg/errors"
	"github.com/segmentio/fasthash/fnv1"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/schema"
	"golang.org/x/time/rate"
)

var requestCounter = metrics.NewRequestMeter("bitbucket", "Total number of requests sent to the Bitbucket API.")

// These fields define the self-imposed Bitbucket rate limit (since Bitbucket Server does
// not have a concept of rate limiting in HTTP response headers).
//
// See https://godoc.org/golang.org/x/time/rate#Limiter for an explanation of these fields.
//
// We chose the limits here based on the fact that Sourcegraph is a heavy consumer of the Bitbucket
// Server API and that a large customer had reported to us their Bitbucket instance receives
// ~100 req/s so it seems reasonable for us to (at max) consume ~8 req/s.
//
// Note that, for comparison, Bitbucket Cloud restricts "List all repositories" requests (which are
// a good portion of our requests) to 1,000/hr, and they restrict "List a user or team's repositories"
// requests (which are roughly equal to our repository lookup requests) to 1,000/hr. We perform a list
// repositories request for every 1000 repositories on Bitbucket every 1m by default, so for someone
// with 20,000 Bitbucket repositories we need 20,000/1000 requests per minute (1200/hr) + overhead for
// repository lookup requests by users, and requests for identifying which repositories a user has
// access to (if authorization is in use) and requests for campaign synchronization if it is in use.
//
// These are our default values, they can be changed in configuration
const (
	defaultRateLimit      = rate.Limit(8) // 480/min or 28,800/hr
	defaultRateLimitBurst = 500
)

// Client access a Bitbucket Server via the REST API.
type Client struct {
	// HTTP Client used to communicate with the API
	httpClient httpcli.Doer

	// URL is the base URL of Bitbucket Server.
	URL *url.URL

	// Token is the personal access token for accessing the
	// server. https://bitbucket.example.com/plugins/servlet/access-tokens/manage
	Token string

	// The username and password credentials for accessing the server. Typically these are only
	// used when the server doesn't support personal access tokens (such as Bitbucket Server
	// version 5.4 and older). If both Token and Username/Password are specified, Token is used.
	Username, Password string

	// RateLimit is the self-imposed rate limiter (since Bitbucket does not have a concept
	// of rate limiting in HTTP response headers).
	RateLimit *rate.Limiter

	// OAuth client used to authenticate requests, if set via SetOAuth.
	// Takes precedence over Token and Username / Password authentication.
	Oauth *oauth.Client
}

// NewClient returns an authenticated Bitbucket Server API client with
// the provided configuration. If a nil httpClient is provided, http.DefaultClient
// will be used.
func NewClient(c *schema.BitbucketServerConnection, httpClient httpcli.Doer) (*Client, error) {
	u, err := url.Parse(c.Url)
	if err != nil {
		return nil, err
	}

	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	httpClient = requestCounter.Doer(httpClient, categorize)
	u = extsvc.NormalizeBaseURL(u)

	// Normally our registry will return a default infinite limiter when nothing has been
	// synced from config. However, we always want to ensure there is at least some form of rate
	// limiting for Bitbucket.
	defaultLimiter := rate.NewLimiter(defaultRateLimit, defaultRateLimitBurst)
	l := ratelimit.DefaultRegistry.GetOrSet(u.String(), defaultLimiter)

	client := &Client{
		httpClient: httpClient,
		URL:        u,
		Username:   c.Username,
		Password:   c.Password,
		Token:      c.Token,
		RateLimit:  l,
	}

	if c.Authorization != nil {
		err := client.SetOAuth(
			c.Authorization.Oauth.ConsumerKey,
			c.Authorization.Oauth.SigningKey,
		)
		if err != nil {
			return nil, errors.Wrap(err, "authorization.oauth.signingKey")
		}
	}

	return client, nil
}

// SetOAuth enables OAuth authentication in a Client, using the given consumer
// key to identify with the Bitbucket Server API and the request signing RSA key
// to authenticate requests. It parses the given Base64 encoded PEM encoded private key,
// returning an error in case of failure.
//
// When using OAuth authentication, it's possible to impersonate any Bitbucket
// Server API user by passing a ?user_id=$username query parameter. This requires
// the Application Link in the Bitbucket Server API to be configured with 2 legged
// OAuth and for it to allow user impersonation.
func (c *Client) SetOAuth(consumerKey, signingKey string) error {
	pemKey, err := base64.StdEncoding.DecodeString(signingKey)
	if err != nil {
		return err
	}

	block, _ := pem.Decode(pemKey)
	if block == nil {
		return errors.New("failed to parse PEM block containing the signing key")
	}

	key, err := x509.ParsePKCS1PrivateKey(block.Bytes)
	if err != nil {
		return err
	}

	c.Oauth = &oauth.Client{
		Credentials:     oauth.Credentials{Token: consumerKey},
		PrivateKey:      key,
		SignatureMethod: oauth.RSASHA1,
	}

	return nil
}

// Sudo returns a copy of the Client authenticated as the Bitbucket Server user with
// the given username. This only works when using OAuth authentication and if the
// Application Link in Bitbucket Server is configured to allow user impersonation,
// returning an error otherwise.
func (c *Client) Sudo(username string) (*Client, error) {
	if c.Oauth == nil {
		return nil, errors.New("bitbucketserver.Client: OAuth not configured")
	}

	sudo := *c
	sudo.Username = username
	return &sudo, nil
}

// UserFilters is a list of UserFilter that is ANDed together.
type UserFilters []UserFilter

// EncodeTo encodes the UserFilter to the given url.Values.
func (fs UserFilters) EncodeTo(qry url.Values) {
	var perm int
	for _, f := range fs {
		if f.Permission != (PermissionFilter{}) {
			perm++
			f.Permission.index = perm
		}
		f.EncodeTo(qry)
	}
}

// UserFilter defines a sum type of filters to be used when listing users.
type UserFilter struct {
	// Filter filters the returned users to those whose username,
	// name or email address contain this value.
	// The API doesn't support exact matches.
	Filter string
	// Group filters the returned users to those who are in the give group.
	Group string
	// Permission filters the returned users to those having the given
	// permissions.
	Permission PermissionFilter
}

// EncodeTo encodes the UserFilter to the given url.Values.
func (f UserFilter) EncodeTo(qry url.Values) {
	if f.Filter != "" {
		qry.Set("filter", f.Filter)
	}

	if f.Group != "" {
		qry.Set("group", f.Group)
	}

	if f.Permission != (PermissionFilter{}) {
		f.Permission.EncodeTo(qry)
	}
}

// A PermissionFilter is a filter used to list users that have specific
// permissions.
type PermissionFilter struct {
	Root           Perm
	ProjectID      string
	ProjectKey     string
	RepositoryID   string
	RepositorySlug string

	index int
}

// EncodeTo encodes the PermissionFilter to the given url.Values.
func (p PermissionFilter) EncodeTo(qry url.Values) {
	q := "permission"

	if p.index != 0 {
		q += "." + strconv.Itoa(p.index)
	}

	qry.Set(q, string(p.Root))

	if p.ProjectID != "" {
		qry.Set(q+".projectId", p.ProjectID)
	}

	if p.ProjectKey != "" {
		qry.Set(q+".projectKey", p.ProjectKey)
	}

	if p.RepositoryID != "" {
		qry.Set(q+".repositoryId", p.RepositoryID)
	}

	if p.RepositorySlug != "" {
		qry.Set(q+".repositorySlug", p.RepositorySlug)
	}
}

// ErrUserFiltersLimit is returned by Client.Users when the UserFiltersLimit is exceeded.
var ErrUserFiltersLimit = errors.Errorf("maximum of %d user filters exceeded", userFiltersLimit)

// userFiltersLimit defines the maximum number of UserFilters that can
// be passed to a single Client.Users call.
const userFiltersLimit = 50

// Users retrieves a page of users, optionally run through provided filters.
func (c *Client) Users(ctx context.Context, pageToken *PageToken, fs ...UserFilter) ([]*User, *PageToken, error) {
	if len(fs) > userFiltersLimit {
		return nil, nil, ErrUserFiltersLimit
	}

	qry := make(url.Values)
	UserFilters(fs).EncodeTo(qry)

	var users []*User
	next, err := c.page(ctx, "rest/api/1.0/users", qry, pageToken, &users)
	return users, next, err
}

// UserPermissions retrieves the global permissions assigned to the user with the given
// username. Used to validate that the client is authenticated as an admin.
func (c *Client) UserPermissions(ctx context.Context, username string) (perms []Perm, _ error) {
	qry := url.Values{"filter": {username}}

	type permission struct {
		User       *User `json:"user"`
		Permission Perm  `json:"permission"`
	}

	var ps []permission
	err := c.send(ctx, "GET", "rest/api/1.0/admin/permissions/users", qry, nil, &struct {
		Values []permission `json:"values"`
	}{
		Values: ps,
	})
	if err != nil {
		return nil, err
	}

	for _, p := range ps {
		if p.User.Name == username {
			perms = append(perms, p.Permission)
		}
	}

	return perms, nil
}

// CreateUser creates the given User returning an error in case of failure.
func (c *Client) CreateUser(ctx context.Context, u *User) error {
	qry := url.Values{
		"name":              {u.Name},
		"password":          {u.Password},
		"displayName":       {u.DisplayName},
		"emailAddress":      {u.EmailAddress},
		"addToDefaultGroup": {"true"},
	}

	return c.send(ctx, "POST", "rest/api/1.0/admin/users", qry, nil, nil)
}

// LoadUser loads the given User returning an error in case of failure.
func (c *Client) LoadUser(ctx context.Context, u *User) error {
	return c.send(ctx, "GET", "rest/api/1.0/users/"+u.Slug, nil, nil, u)
}

// LoadGroup loads the given Group returning an error in case of failure.
func (c *Client) LoadGroup(ctx context.Context, g *Group) error {
	qry := url.Values{"filter": {g.Name}}
	var groups struct {
		Values []*Group `json:"values"`
	}

	err := c.send(ctx, "GET", "rest/api/1.0/admin/groups", qry, nil, &groups)
	if err != nil {
		return err
	}

	if len(groups.Values) != 1 {
		return errors.New("group not found")
	}

	*g = *groups.Values[0]

	return nil
}

// CreateGroup creates the given Group returning an error in case of failure.
func (c *Client) CreateGroup(ctx context.Context, g *Group) error {
	qry := url.Values{"name": {g.Name}}
	return c.send(ctx, "POST", "rest/api/1.0/admin/groups", qry, g, g)
}

// CreateGroupMembership creates the given Group's membership returning an error in case of failure.
func (c *Client) CreateGroupMembership(ctx context.Context, g *Group) error {
	type membership struct {
		Group string   `json:"group"`
		Users []string `json:"users"`
	}
	m := &membership{Group: g.Name, Users: g.Users}
	return c.send(ctx, "POST", "rest/api/1.0/admin/groups/add-users", nil, m, nil)
}

// CreateUserRepoPermission creates the given permission returning an error in case of failure.
func (c *Client) CreateUserRepoPermission(ctx context.Context, p *UserRepoPermission) error {
	path := "rest/api/1.0/projects/" + p.Repo.Project.Key + "/repos/" + p.Repo.Slug + "/permissions/users"
	return c.createPermission(ctx, path, p.User.Name, p.Perm)
}

// CreateUserProjectPermission creates the given permission returning an error in case of failure.
func (c *Client) CreateUserProjectPermission(ctx context.Context, p *UserProjectPermission) error {
	path := "rest/api/1.0/projects/" + p.Project.Key + "/permissions/users"
	return c.createPermission(ctx, path, p.User.Name, p.Perm)
}

// CreateGroupProjectPermission creates the given permission returning an error in case of failure.
func (c *Client) CreateGroupProjectPermission(ctx context.Context, p *GroupProjectPermission) error {
	path := "rest/api/1.0/projects/" + p.Project.Key + "/permissions/groups"
	return c.createPermission(ctx, path, p.Group.Name, p.Perm)
}

// CreateGroupRepoPermission creates the given permission returning an error in case of failure.
func (c *Client) CreateGroupRepoPermission(ctx context.Context, p *GroupRepoPermission) error {
	path := "rest/api/1.0/projects/" + p.Repo.Project.Key + "/repos/" + p.Repo.Slug + "/permissions/groups"
	return c.createPermission(ctx, path, p.Group.Name, p.Perm)
}

func (c *Client) createPermission(ctx context.Context, path, name string, p Perm) error {
	qry := url.Values{
		"name":       {name},
		"permission": {string(p)},
	}
	return c.send(ctx, "PUT", path, qry, nil, nil)
}

// CreateRepo creates the given Repo returning an error in case of failure.
func (c *Client) CreateRepo(ctx context.Context, r *Repo) error {
	path := "rest/api/1.0/projects/" + r.Project.Key + "/repos"
	return c.send(ctx, "POST", path, nil, r, &struct {
		Values []*Repo `json:"values"`
	}{
		Values: []*Repo{r},
	})
}

// LoadProject loads the given Project returning an error in case of failure.
func (c *Client) LoadProject(ctx context.Context, p *Project) error {
	return c.send(ctx, "GET", "rest/api/1.0/projects/"+p.Key, nil, nil, p)
}

// CreateProject creates the given Project returning an error in case of failure.
func (c *Client) CreateProject(ctx context.Context, p *Project) error {
	return c.send(ctx, "POST", "rest/api/1.0/projects", nil, p, p)
}

// LoadPullRequest loads the given PullRequest returning an error in case of failure.
func (c *Client) LoadPullRequest(ctx context.Context, pr *PullRequest) error {
	if pr.ToRef.Repository.Slug == "" {
		return errors.New("repository slug empty")
	}
	if pr.ToRef.Repository.Project.Key == "" {
		return errors.New("project key empty")
	}

	path := fmt.Sprintf(
		"rest/api/1.0/projects/%s/repos/%s/pull-requests/%d",
		pr.ToRef.Repository.Project.Key,
		pr.ToRef.Repository.Slug,
		pr.ID,
	)
	return c.send(ctx, "GET", path, nil, nil, pr)
}

type UpdatePullRequestInput struct {
	PullRequestID string `json:"-"`
	Version       int    `json:"version"`

	Title       string `json:"title"`
	Description string `json:"description"`
	ToRef       Ref    `json:"toRef"`
}

func (c *Client) UpdatePullRequest(ctx context.Context, in *UpdatePullRequestInput) (*PullRequest, error) {
	path := fmt.Sprintf(
		"rest/api/1.0/projects/%s/repos/%s/pull-requests/%s",
		in.ToRef.Repository.Project.Key,
		in.ToRef.Repository.Slug,
		in.PullRequestID,
	)

	pr := &PullRequest{}
	return pr, c.send(ctx, "PUT", path, nil, in, pr)
}

// ErrAlreadyExists is returned by Client.CreatePullRequest when a Pull Request
// for the given FromRef and ToRef already exists.
type ErrAlreadyExists struct {
	Existing *PullRequest
}

func (e ErrAlreadyExists) Error() string {
	return "A pull request with the given to and from refs already exists"
}

// CreatePullRequest creates the given PullRequest returning an error in case of failure.
func (c *Client) CreatePullRequest(ctx context.Context, pr *PullRequest) error {
	for _, namedRef := range [...]struct {
		name string
		ref  Ref
	}{
		{"ToRef", pr.ToRef},
		{"FromRef", pr.FromRef},
	} {
		if namedRef.ref.ID == "" {
			return errors.Errorf("%s id empty", namedRef.name)
		}
		if namedRef.ref.Repository.Slug == "" {
			return errors.Errorf("%s repository slug empty", namedRef.name)
		}
		if namedRef.ref.Repository.Project.Key == "" {
			return errors.Errorf("%s project key empty", namedRef.name)
		}
	}

	type requestBody struct {
		Title       string `json:"title"`
		Description string `json:"description"`
		State       string `json:"state"`
		Open        bool   `json:"open"`
		Closed      bool   `json:"closed"`
		FromRef     Ref    `json:"fromRef"`
		ToRef       Ref    `json:"toRef"`
		Locked      bool   `json:"locked"`
	}

	// Bitbucket Server doesn't support GFM taskitems. But since we might add
	// those to a PR description for certain Automation Campaigns, we have to
	// "downgrade" here and for now, removing taskitems is enough.
	description := strings.ReplaceAll(pr.Description, "- [ ] ", "- ")

	payload := requestBody{
		Title:       pr.Title,
		Description: description,
		State:       "OPEN",
		Open:        true,
		Closed:      false,
		FromRef:     pr.FromRef,
		ToRef:       pr.ToRef,
		Locked:      false,
	}

	path := fmt.Sprintf(
		"rest/api/1.0/projects/%s/repos/%s/pull-requests",
		pr.ToRef.Repository.Project.Key,
		pr.ToRef.Repository.Slug,
	)

	err := c.send(ctx, "POST", path, nil, payload, pr)
	if err != nil {
		if IsDuplicatePullRequest(err) {
			pr, extractErr := ExtractDuplicatePullRequest(err)
			if extractErr != nil {
				log15.Error("Extracting existsing PR", "err", extractErr)
			}
			return &ErrAlreadyExists{
				Existing: pr,
			}
		}
		return err
	}
	return nil
}

// DeclinePullRequest declines and closes the given PullRequest, returning an error in case of failure.
func (c *Client) DeclinePullRequest(ctx context.Context, pr *PullRequest) error {
	if pr.ToRef.Repository.Slug == "" {
		return errors.New("repository slug empty")
	}

	if pr.ToRef.Repository.Project.Key == "" {
		return errors.New("project key empty")
	}

	path := fmt.Sprintf(
		"rest/api/1.0/projects/%s/repos/%s/pull-requests/%d/decline",
		pr.ToRef.Repository.Project.Key,
		pr.ToRef.Repository.Slug,
		pr.ID,
	)

	qry := url.Values{"version": {strconv.Itoa(pr.Version)}}

	return c.send(ctx, "POST", path, qry, nil, pr)
}

// LoadPullRequestActivities loads the given PullRequest's timeline of activities,
// returning an error in case of failure.
func (c *Client) LoadPullRequestActivities(ctx context.Context, pr *PullRequest) (err error) {
	if pr.ToRef.Repository.Slug == "" {
		return errors.New("repository slug empty")
	}

	if pr.ToRef.Repository.Project.Key == "" {
		return errors.New("project key empty")
	}

	path := fmt.Sprintf(
		"rest/api/1.0/projects/%s/repos/%s/pull-requests/%d/activities",
		pr.ToRef.Repository.Project.Key,
		pr.ToRef.Repository.Slug,
		pr.ID,
	)

	t := &PageToken{Limit: 1000}

	var activities []*Activity
	for t.HasMore() {
		var page []*Activity
		if t, err = c.page(ctx, path, nil, t, &page); err != nil {
			return err
		}
		activities = append(activities, page...)
	}

	pr.Activities = activities
	return nil
}

func (c *Client) LoadPullRequestCommits(ctx context.Context, pr *PullRequest) (err error) {
	if pr.ToRef.Repository.Slug == "" {
		return errors.New("repository slug empty")
	}

	if pr.ToRef.Repository.Project.Key == "" {
		return errors.New("project key empty")
	}

	path := fmt.Sprintf(
		"rest/api/1.0/projects/%s/repos/%s/pull-requests/%d/commits",
		pr.ToRef.Repository.Project.Key,
		pr.ToRef.Repository.Slug,
		pr.ID,
	)

	t := &PageToken{Limit: 1000}

	var commits []*Commit
	for t.HasMore() {
		var page []*Commit
		if t, err = c.page(ctx, path, nil, t, &page); err != nil {
			return err
		}
		commits = append(commits, page...)
	}

	pr.Commits = commits
	return nil
}

func (c *Client) LoadPullRequestBuildStatuses(ctx context.Context, pr *PullRequest) (err error) {
	if len(pr.Commits) == 0 {
		return nil
	}

	var latestCommit Commit
	for _, c := range pr.Commits {
		if latestCommit.CommitterTimestamp < c.CommitterTimestamp {
			latestCommit = *c
		}
	}

	path := fmt.Sprintf("rest/build-status/1.0/commits/%s", latestCommit.ID)

	t := &PageToken{Limit: 1000}

	var statuses []*CommitStatus
	for t.HasMore() {
		var page []*BuildStatus
		if t, err = c.page(ctx, path, nil, t, &page); err != nil {
			return err
		}
		for i := range page {
			status := &CommitStatus{
				Commit: latestCommit.ID,
				Status: *page[i],
			}
			statuses = append(statuses, status)
		}
	}

	pr.CommitStatus = statuses
	return nil
}

func (c *Client) Repo(ctx context.Context, projectKey, repoSlug string) (*Repo, error) {
	u := fmt.Sprintf("rest/api/1.0/projects/%s/repos/%s", projectKey, repoSlug)
	req, err := http.NewRequest("GET", u, nil)
	if err != nil {
		return nil, err
	}
	var resp Repo
	err = c.do(ctx, req, &resp)
	return &resp, err
}

func (c *Client) Repos(ctx context.Context, pageToken *PageToken, searchQueries ...string) ([]*Repo, *PageToken, error) {
	qry, err := parseQueryStrings(searchQueries...)
	if err != nil {
		return nil, pageToken, err
	}

	var repos []*Repo
	next, err := c.page(ctx, "rest/api/1.0/repos", qry, pageToken, &repos)
	return repos, next, err
}

func (c *Client) LabeledRepos(ctx context.Context, pageToken *PageToken, label string) ([]*Repo, *PageToken, error) {
	u := fmt.Sprintf("rest/api/1.0/labels/%s/labeled", label)
	qry := url.Values{
		"REPOSITORY": []string{""},
	}

	var repos []*Repo
	next, err := c.page(ctx, u, qry, pageToken, &repos)
	return repos, next, err
}

// RepoIDs fetches a list of repository IDs that the user token has permission for.
// Permission: ["admin", "read", "write"]
func (c *Client) RepoIDs(ctx context.Context, permission string) ([]uint32, error) {
	u := fmt.Sprintf("rest/sourcegraph-admin/1.0/permissions/repositories?permission=%s", permission)
	req, err := http.NewRequest("GET", u, nil)
	if err != nil {
		return nil, err
	}
	var resp []byte
	err = c.do(ctx, req, &resp)
	if err != nil {
		return nil, err
	}

	bitmap := roaring.New()
	if err := bitmap.UnmarshalBinary(resp); err != nil {
		return nil, err
	}
	return bitmap.ToArray(), nil
}

func (c *Client) RecentRepos(ctx context.Context, pageToken *PageToken) ([]*Repo, *PageToken, error) {
	var repos []*Repo
	next, err := c.page(ctx, "rest/api/1.0/profile/recent/repos", nil, pageToken, &repos)
	return repos, next, err
}

func (c *Client) page(ctx context.Context, path string, qry url.Values, token *PageToken, results interface{}) (*PageToken, error) {
	if qry == nil {
		qry = make(url.Values)
	}

	for k, vs := range token.Values() {
		qry[k] = append(qry[k], vs...)
	}

	u := url.URL{Path: path, RawQuery: qry.Encode()}
	req, err := http.NewRequest("GET", u.String(), nil)
	if err != nil {
		return nil, err
	}

	var next PageToken
	err = c.do(ctx, req, &struct {
		*PageToken
		Values interface{} `json:"values"`
	}{
		PageToken: &next,
		Values:    results,
	})

	if err != nil {
		return nil, err
	}

	return &next, nil
}

func (c *Client) send(ctx context.Context, method, path string, qry url.Values, payload, result interface{}) error {
	if qry == nil {
		qry = make(url.Values)
	}

	var body io.ReadWriter
	if payload != nil {
		body = new(bytes.Buffer)
		if err := json.NewEncoder(body).Encode(payload); err != nil {
			return err
		}
	}

	u := url.URL{Path: path, RawQuery: qry.Encode()}
	req, err := http.NewRequest(method, u.String(), body)
	if err != nil {
		return err
	}

	return c.do(ctx, req, result)
}

func (c *Client) do(ctx context.Context, req *http.Request, result interface{}) error {
	req.URL = c.URL.ResolveReference(req.URL)
	if req.Header.Get("Content-Type") == "" {
		req.Header.Set("Content-Type", "application/json; charset=utf-8")
	}

	req, ht := nethttp.TraceRequest(ot.GetTracer(ctx),
		req.WithContext(ctx),
		nethttp.OperationName("Bitbucket Server"),
		nethttp.ClientTrace(false))
	defer ht.Finish()

	if err := c.authenticate(req); err != nil {
		return err
	}

	startWait := time.Now()
	if err := c.RateLimit.Wait(ctx); err != nil {
		return err
	}

	if d := time.Since(startWait); d > 200*time.Millisecond {
		log15.Warn("Bitbucket self-enforced API rate limit: request delayed longer than expected due to rate limit", "delay", d)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return err
	}

	defer resp.Body.Close()

	bs, err := ioutil.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return errors.WithStack(&httpError{
			URL:        req.URL,
			StatusCode: resp.StatusCode,
			Body:       bs,
		})
	}

	// handle binary response
	if s, ok := result.(*[]byte); ok {
		*s = bs
	} else if result != nil {
		return json.Unmarshal(bs, result)
	}

	return nil
}

func (c *Client) authenticate(req *http.Request) error {
	// Authenticate request, in order of preference.
	if c.Oauth != nil {
		if c.Username != "" {
			qry := req.URL.Query()
			qry.Set("user_id", c.Username)
			req.URL.RawQuery = qry.Encode()
		}

		if err := c.Oauth.SetAuthorizationHeader(
			req.Header,
			&oauth.Credentials{Token: ""}, // Token must be empty
			req.Method,
			req.URL,
			nil,
		); err != nil {
			return err
		}
	} else if c.Token != "" {
		req.Header.Set("Authorization", "Bearer "+c.Token)
	} else if c.Username != "" || c.Password != "" {
		req.SetBasicAuth(c.Username, c.Password)
	}

	return nil
}

func parseQueryStrings(qs ...string) (url.Values, error) {
	vals := make(url.Values)
	for _, q := range qs {
		query, err := url.ParseQuery(strings.TrimPrefix(q, "?"))
		if err != nil {
			return nil, err
		}
		for k, vs := range query {
			vals[k] = append(vals[k], vs...)
		}
	}
	return vals, nil
}

// categorize returns a category for an API URL. Used by metrics.
func categorize(u *url.URL) string {
	// API to URL mapping looks like this:
	//
	// 	Repo -> rest/api/1.0/profile/recent/repos%s
	// 	Repos -> rest/api/1.0/projects/%s/repos/%s
	// 	RecentRepos -> rest/api/1.0/repos%s
	//
	// We guess the category based on the fourth path component ("profile", "projects", "repos%s").
	var category string
	if parts := strings.SplitN(u.Path, "/", 3); len(parts) >= 4 {
		category = parts[3]
	}
	switch {
	case category == "profile":
		return "Repo"
	case category == "projects":
		return "Repos"
	case strings.HasPrefix(category, "repos"):
		return "RecentRepos"
	default:
		// don't return category directly as that could introduce too much dimensionality
		return "unknown"
	}
}

type PageToken struct {
	Size          int  `json:"size"`
	Limit         int  `json:"limit"`
	IsLastPage    bool `json:"isLastPage"`
	Start         int  `json:"start"`
	NextPageStart int  `json:"nextPageStart"`
}

func (t *PageToken) HasMore() bool {
	if t == nil {
		return true
	}
	return !t.IsLastPage
}

func (t *PageToken) Query() string {
	if t == nil {
		return ""
	}
	v := t.Values()
	if len(v) == 0 {
		return ""
	}
	return "?" + v.Encode()
}

func (t *PageToken) Values() url.Values {
	v := url.Values{}
	if t == nil {
		return v
	}
	if t.NextPageStart != 0 {
		v.Set("start", strconv.Itoa(t.NextPageStart))
	}
	if t.Limit != 0 {
		v.Set("limit", strconv.Itoa(t.Limit))
	}
	return v
}

// Perm represents a Bitbucket Server permission.
type Perm string

// Permission constants.
const (
	PermSysAdmin      Perm = "SYS_ADMIN"
	PermAdmin         Perm = "ADMIN"
	PermLicensedUser  Perm = "LICENSED_USER"
	PermProjectCreate Perm = "PROJECT_CREATE"

	PermProjectAdmin Perm = "PROJECT_ADMIN"
	PermProjectWrite Perm = "PROJECT_WRITE"
	PermProjectView  Perm = "PROJECT_VIEW"
	PermProjectRead  Perm = "PROJECT_READ"

	PermRepoAdmin Perm = "REPO_ADMIN"
	PermRepoRead  Perm = "REPO_READ"
	PermRepoWrite Perm = "REPO_WRITE"
)

// User account in a Bitbucket Server instance.
type User struct {
	Name         string `json:"name,omitempty"`
	Password     string `json:"-"`
	EmailAddress string `json:"emailAddress,omitempty"`
	ID           int    `json:"id,omitempty"`
	DisplayName  string `json:"displayName,omitempty"`
	Active       bool   `json:"active,omitempty"`
	Slug         string `json:"slug,omitempty"`
	Type         string `json:"type,omitempty"`
}

// Group of users in a Bitbucket Server instance.
type Group struct {
	Name  string   `json:"name,omitempty"`
	Users []string `json:"users,omitempty"`
}

// A UserRepoPermission of a User to perform certain actions
// on a Repo.
type UserRepoPermission struct {
	User *User
	Perm Perm
	Repo *Repo
}

// A GroupRepoPermission of a Group to perform certain actions
// on a Repo.
type GroupRepoPermission struct {
	Group *Group
	Perm  Perm
	Repo  *Repo
}

// A UserProjectPermission of a User to perform certain actions
// on a Project.
type UserProjectPermission struct {
	User    *User
	Perm    Perm
	Project *Project
}

// A GroupProjectPermission of a Group to perform certain actions
// on a Project.
type GroupProjectPermission struct {
	Group   *Group
	Perm    Perm
	Project *Project
}

type Repo struct {
	Slug          string   `json:"slug"`
	ID            int      `json:"id"`
	Name          string   `json:"name"`
	SCMID         string   `json:"scmId"`
	State         string   `json:"state"`
	StatusMessage string   `json:"statusMessage"`
	Forkable      bool     `json:"forkable"`
	Origin        *Repo    `json:"origin"`
	Project       *Project `json:"project"`
	Public        bool     `json:"public"`
	Links         struct {
		Clone []struct {
			Href string `json:"href"`
			Name string `json:"name"`
		} `json:"clone"`
		Self []struct {
			Href string `json:"href"`
		} `json:"self"`
	} `json:"links"`
}

// IsPersonalRepository tells if the repository is a personal one.
func (r *Repo) IsPersonalRepository() bool {
	return r.Project.Type == "PERSONAL"
}

type Project struct {
	Key    string `json:"key"`
	ID     int    `json:"id"`
	Name   string `json:"name"`
	Public bool   `json:"public"`
	Type   string `json:"type"`
	Links  struct {
		Self []struct {
			Href string `json:"href"`
		} `json:"self"`
	} `json:"links"`
}

type Ref struct {
	ID         string `json:"id"`
	Repository struct {
		ID      int    `json:"id"`
		Slug    string `json:"slug"`
		Project struct {
			Key string `json:"key"`
		} `json:"project"`
	} `json:"repository"`
}

type PullRequest struct {
	ID          int    `json:"id"`
	Version     int    `json:"version"`
	Title       string `json:"title"`
	Description string `json:"description"`
	State       string `json:"state"`
	Open        bool   `json:"open"`
	Closed      bool   `json:"closed"`
	CreatedDate int    `json:"createdDate"`
	UpdatedDate int    `json:"updatedDate"`
	FromRef     Ref    `json:"fromRef"`
	ToRef       Ref    `json:"toRef"`
	Locked      bool   `json:"locked"`
	Author      struct {
		User     *User  `json:"user"`
		Role     string `json:"role"`
		Approved bool   `json:"approved"`
		Status   string `json:"status"`
	} `json:"author"`
	Reviewers []struct {
		User               *User  `json:"user"`
		LastReviewedCommit string `json:"lastReviewedCommit"`
		Role               string `json:"role"`
		Approved           bool   `json:"approved"`
		Status             string `json:"status"`
	} `json:"reviewers"`
	Participants []struct {
		User     *User  `json:"user"`
		Role     string `json:"role"`
		Approved bool   `json:"approved"`
		Status   string `json:"status"`
	} `json:"participants"`
	Links struct {
		Self []struct {
			Href string `json:"href"`
		} `json:"self"`
	} `json:"links"`

	Activities   []*Activity     `json:"activities,omitempty"`
	Commits      []*Commit       `json:"commits,omitempty"`
	CommitStatus []*CommitStatus `json:"commit_status,omitempty"`

	// Deprecated, use CommitStatus instead. BuildStatus was not tied to individual commits
	BuildStatuses []*BuildStatus `json:"buildstatuses,omitempty"`
}

// Activity is a union type of all supported pull request activity items.
type Activity struct {
	ID          int            `json:"id"`
	CreatedDate int            `json:"createdDate"`
	User        User           `json:"user"`
	Action      ActivityAction `json:"action"`

	// Comment activity fields.
	CommentAction string         `json:"commentAction,omitempty"`
	Comment       *Comment       `json:"comment,omitempty"`
	CommentAnchor *CommentAnchor `json:"commentAnchor,omitempty"`

	// Reviewers change fields.
	AddedReviewers   []User `json:"addedReviewers,omitempty"`
	RemovedReviewers []User `json:"removedReviewers,omitempty"`

	// Merged event fields.
	Commit *Commit `json:"commit,omitempty"`
}

// Key is a unique key identifying this activity in the context of its pull request.
func (a *Activity) Key() string { return strconv.Itoa(a.ID) }

// BuildStatus represents the build status of a commit
type BuildStatus struct {
	State       string `json:"state,omitempty"`
	Key         string `json:"key,omitempty"`
	Name        string `json:"name,omitempty"`
	Url         string `json:"url,omitempty"`
	Description string `json:"description,omitempty"`
	DateAdded   int64  `json:"dateAdded,omitempty"`
}

// Commit status is the build status for a specific commit
type CommitStatus struct {
	Commit string      `json:"commit,omitempty"`
	Status BuildStatus `json:"status,omitempty"`
}

func (s *CommitStatus) Key() string {
	key := fmt.Sprintf("%s:%s:%s:%s", s.Commit, s.Status.Key, s.Status.Name, s.Status.Url)
	return strconv.FormatInt(int64(fnv1.HashString64(key)), 16)
}

// ActivityAction defines the action taken in an Activity.
type ActivityAction string

// Known ActivityActions
const (
	ApprovedActivityAction   ActivityAction = "APPROVED"
	UnapprovedActivityAction ActivityAction = "UNAPPROVED"
	DeclinedActivityAction   ActivityAction = "DECLINED"
	ReviewedActivityAction   ActivityAction = "REVIEWED"
	OpenedActivityAction     ActivityAction = "OPENED"
	ReopenedActivityAction   ActivityAction = "REOPENED"
	RescopedActivityAction   ActivityAction = "RESCOPED"
	UpdatedActivityAction    ActivityAction = "UPDATED"
	CommentedActivityAction  ActivityAction = "COMMENTED"
	MergedActivityAction     ActivityAction = "MERGED"
)

// A Comment in a PullRequest.
type Comment struct {
	ID                  int                 `json:"id"`
	Version             int                 `json:"version"`
	Text                string              `json:"text"`
	Author              User                `json:"author"`
	CreatedDate         int                 `json:"createdDate"`
	UpdatedDate         int                 `json:"updatedDate"`
	Comments            []Comment           `json:"comments"` // Replies to the comment
	Tasks               []Task              `json:"tasks"`
	PermittedOperations PermittedOperations `json:"permittedOperations"`
}

// A CommentAnchor captures the location of a code comment in a PullRequest.
type CommentAnchor struct {
	FromHash string `json:"fromHash"`
	ToHash   string `json:"toHash"`
	Line     int    `json:"line"`
	LineType string `json:"lineType"`
	FileType string `json:"fileType"`
	Path     string `json:"path"`
	DiffType string `json:"diffType"`
	Orphaned bool   `json:"orphaned"`
}

// A Task in a PullRequest.
type Task struct {
	ID                  int                 `json:"id"`
	Author              User                `json:"author"`
	Text                string              `json:"text"`
	State               string              `json:"state"`
	CreatedDate         int                 `json:"createdDate"`
	PermittedOperations PermittedOperations `json:"permittedOperations"`
}

// PermittedOperations of a Comment or Task.
type PermittedOperations struct {
	Editable       bool `json:"editable,omitempty"`
	Deletable      bool `json:"deletable,omitempty"`
	Transitionable bool `json:"transitionable,omitempty"`
}

// A Commit in a Repository.
type Commit struct {
	ID                 string   `json:"id,omitempty"`
	DisplayID          string   `json:"displayId,omitempty"`
	Author             *User    `json:"user,omitempty"`
	AuthorTimestamp    int64    `json:"authorTimestamp,omitempty"`
	Committer          *User    `json:"committer,omitempty"`
	CommitterTimestamp int64    `json:"committerTimestamp,omitempty"`
	Message            string   `json:"message,omitempty"`
	Parents            []Commit `json:"parents,omitempty"`
}

// IsNotFound reports whether err is a Bitbucket Server API not found error.
func IsNotFound(err error) bool {
	switch e := errors.Cause(err).(type) {
	case *httpError:
		return e.NotFound()
	}
	return false
}

// IsNoSuchLabel reports whether err is a Bitbucket Server API "No Such Label"
// error.
func IsNoSuchLabel(err error) bool {
	switch e := errors.Cause(err).(type) {
	case *httpError:
		return e.NoSuchLabelException()
	}
	return false
}

// IsDuplicatePullRequest reports whether err is a Bitbucket Server API
// "Duplicate Pull Request" error.
func IsDuplicatePullRequest(err error) bool {
	switch e := errors.Cause(err).(type) {
	case *httpError:
		return e.DuplicatePullRequest()
	}
	return false
}

// ExtractDuplicatePullRequest will attempt to extract a duplicate PR
func ExtractDuplicatePullRequest(err error) (*PullRequest, error) {
	switch e := errors.Cause(err).(type) {
	case *httpError:
		return e.ExtractExistingPullRequest()
	}
	return nil, fmt.Errorf("error does not contain existing PR")
}

type httpError struct {
	StatusCode int
	URL        *url.URL
	Body       []byte
}

func (e *httpError) Error() string {
	return fmt.Sprintf("Bitbucket API HTTP error: code=%d url=%q body=%q", e.StatusCode, e.URL, e.Body)
}

func (e *httpError) Unauthorized() bool {
	return e.StatusCode == http.StatusUnauthorized
}

func (e *httpError) NotFound() bool {
	return e.StatusCode == http.StatusNotFound
}

func (e *httpError) DuplicatePullRequest() bool {
	return strings.Contains(string(e.Body), bitbucketDuplicatePRException)
}

func (e *httpError) NoSuchLabelException() bool {
	return strings.Contains(string(e.Body), bitbucketNoSuchLabelException)
}

const (
	bitbucketDuplicatePRException = "com.atlassian.bitbucket.pull.DuplicatePullRequestException"
	bitbucketNoSuchLabelException = "com.atlassian.bitbucket.label.NoSuchLabelException"
)

func (e *httpError) ExtractExistingPullRequest() (*PullRequest, error) {
	var dest struct {
		Errors []struct {
			ExceptionName       string
			ExistingPullRequest PullRequest
		}
	}

	err := json.Unmarshal(e.Body, &dest)
	if err != nil {
		return nil, errors.Wrap(err, "unmarshalling error")
	}

	for _, e := range dest.Errors {
		if e.ExceptionName == bitbucketDuplicatePRException {
			return &e.ExistingPullRequest, nil
		}
	}

	return nil, errors.New("existing PR not found")
}
