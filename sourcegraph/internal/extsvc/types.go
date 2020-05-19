package extsvc

import (
	"encoding/json"
	"fmt"
	"net/url"
	"strings"
	"time"

	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
	"golang.org/x/time/rate"
)

// Account represents a row in the `user_external_accounts` table. See the GraphQL API's
// corresponding fields in "ExternalAccount" for documentation.
type Account struct {
	ID          int32
	UserID      int32
	AccountSpec // ServiceType, ServiceID, ClientID, AccountID
	AccountData // AuthData, Data
	CreatedAt   time.Time
	UpdatedAt   time.Time
}

// AccountSpec specifies a user external account by its external identifier (i.e., by the
// identifier provided by the account's owner service), instead of by our database's serial
// ID. See the GraphQL API's corresponding fields in "ExternalAccount" for documentation.
type AccountSpec struct {
	ServiceType string
	ServiceID   string
	ClientID    string
	AccountID   string
}

// AccountData contains data that can be freely updated in the user external account after it
// has been created. See the GraphQL API's corresponding fields for documentation.
type AccountData struct {
	AuthData *json.RawMessage
	Data     *json.RawMessage
}

// Repository contains necessary information to identify an external repository on the code host.
type Repository struct {
	// URI is the full name for this repository, e.g. "github.com/user/repo".
	URI string
	api.ExternalRepoSpec
}

// Accounts contains a list of accounts that belong to the same external service.
// All fields have a same meaning to AccountSpec. See GraphQL API's corresponding fields
// in "ExternalAccount" for documentation.
type Accounts struct {
	ServiceType string
	ServiceID   string
	AccountIDs  []string
}

// TracingFields returns tracing fields for the opentracing log.
func (s *Accounts) TracingFields() []otlog.Field {
	return []otlog.Field{
		otlog.String("Accounts.ServiceType", s.ServiceType),
		otlog.String("Accounts.Perm", s.ServiceID),
		otlog.Int("Accounts.AccountIDs.Count", len(s.AccountIDs)),
	}
}

// AccountID is a descriptive type for the external identifier of an external account on the
// code host. It can be the string representation of an integer (e.g. GitLab), a GraphQL ID
// (e.g. GitHub), or a username (e.g. Bitbucket Server) depends on the code host type.
type AccountID string

// RepoID is a descriptive type for the external identifier of an external repository on the
// code host. It can be the string representation of an integer (e.g. GitLab and Bitbucket
// Server) or a GraphQL ID (e.g. GitHub) depends on the code host type.
type RepoID string

// ParseConfig attempts to unmarshal the given JSON config into a configuration struct defined in the schema package.
func ParseConfig(kind, config string) (cfg interface{}, _ error) {
	switch strings.ToLower(kind) {
	case "awscodecommit":
		cfg = &schema.AWSCodeCommitConnection{}
	case "bitbucketserver":
		cfg = &schema.BitbucketServerConnection{}
	case "bitbucketcloud":
		cfg = &schema.BitbucketCloudConnection{}
	case "github":
		cfg = &schema.GitHubConnection{}
	case "gitlab":
		cfg = &schema.GitLabConnection{}
	case "gitolite":
		cfg = &schema.GitoliteConnection{}
	case "phabricator":
		cfg = &schema.PhabricatorConnection{}
	case "other":
		cfg = &schema.OtherExternalServiceConnection{}
	default:
		return nil, fmt.Errorf("unknown external service kind %q", kind)
	}
	return cfg, jsonc.Unmarshal(config, cfg)
}

const IDParam = "externalServiceID"

func WebhookURL(kind string, externalServiceID int64, externalURL string) string {
	var path string
	switch strings.ToLower(kind) {
	case "github":
		path = "github-webhooks"
	case "bitbucketserver":
		path = "bitbucket-server-webhooks"
	default:
		return ""
	}
	// eg. https://example.com/.api/github-webhooks?externalServiceID=1
	return fmt.Sprintf("%s/.api/%s?%s=%d", externalURL, path, IDParam, externalServiceID)
}

// ExtractRateLimitConfig extracts the rate limit config from the given args. If rate limiting is not
// supported the error returned will be an ErrRateLimitUnsupported.
func ExtractRateLimitConfig(config, kind, displayName string) (RateLimitConfig, error) {
	parsed, err := ParseConfig(kind, config)
	if err != nil {
		return RateLimitConfig{}, errors.Wrap(err, "loading service configuration")
	}

	rlc, err := GetLimitFromConfig(kind, parsed)
	if err != nil {
		return RateLimitConfig{}, err
	}
	rlc.DisplayName = displayName

	return rlc, nil
}

// RateLimitConfig represents the internal rate limit configured for an external service
type RateLimitConfig struct {
	BaseURL     string
	DisplayName string
	Limit       rate.Limit
	IsDefault   bool
}

// GetLimitFromConfig gets RateLimitConfig from an already parsed config schema.
func GetLimitFromConfig(kind string, config interface{}) (rlc RateLimitConfig, err error) {
	// Rate limit config can be in a few states:
	// 1. Not defined: We fall back to default specified in code.
	// 2. Defined and enabled: We use their defined limit.
	// 3. Defined and disabled: We use an infinite limiter.

	rlc.IsDefault = true
	switch c := config.(type) {
	case *schema.GitLabConnection:
		// 10/s is the default enforced by GitLab on their end
		rlc.Limit = rate.Limit(10)
		if c != nil && c.RateLimit != nil {
			rlc.Limit = limitOrInf(c.RateLimit.Enabled, c.RateLimit.RequestsPerHour)
			rlc.IsDefault = false
		}
		rlc.BaseURL = c.Url
	case *schema.GitHubConnection:
		// 5000 per hour is the default enforced by GitHub on their end
		rlc.Limit = rate.Limit(5000.0 / 3600.0)
		if c != nil && c.RateLimit != nil {
			rlc.Limit = limitOrInf(c.RateLimit.Enabled, c.RateLimit.RequestsPerHour)
			rlc.IsDefault = false
		}
		rlc.BaseURL = c.Url
	case *schema.BitbucketServerConnection:
		// 8/s is the default limit we enforce
		rlc.Limit = rate.Limit(8)
		if c != nil && c.RateLimit != nil {
			rlc.Limit = limitOrInf(c.RateLimit.Enabled, c.RateLimit.RequestsPerHour)
			rlc.IsDefault = false
		}
		rlc.BaseURL = c.Url
	case *schema.BitbucketCloudConnection:
		// 2/s is the default limit we enforce
		rlc.Limit = rate.Limit(2)
		if c != nil && c.RateLimit != nil {
			rlc.Limit = limitOrInf(c.RateLimit.Enabled, c.RateLimit.RequestsPerHour)
			rlc.IsDefault = false
		}
		rlc.BaseURL = c.Url
	default:
		return rlc, ErrRateLimitUnsupported{codehostKind: kind}
	}

	u, err := url.Parse(rlc.BaseURL)
	if err != nil {
		return rlc, errors.Wrap(err, "parsing external service URL")
	}

	rlc.BaseURL = NormalizeBaseURL(u).String()

	return rlc, nil
}

func limitOrInf(enabled bool, perHour float64) rate.Limit {
	if enabled {
		return rate.Limit(perHour / 3600)
	}
	return rate.Inf
}

type ErrRateLimitUnsupported struct {
	codehostKind string
}

func (e ErrRateLimitUnsupported) Error() string {
	return fmt.Sprintf("internal rate limiting not supported for %s", e.codehostKind)
}
