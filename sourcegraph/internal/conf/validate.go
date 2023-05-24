package conf

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/tidwall/gjson"
	"github.com/xeipuuv/gojsonschema"

	"github.com/sourcegraph/sourcegraph/internal/conf/confdefaults"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/hashutil"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

// ignoreLegacyKubernetesFields is the set of field names for which validation errors should be
// ignored. The validation errors occur only because deploy-sourcegraph config merged site config
// and Kubernetes cluster-specific config. This is deprecated. Until we have transitioned fully, we
// suppress validation errors on these fields.
var ignoreLegacyKubernetesFields = map[string]struct{}{
	"alertmanagerConfig":    {},
	"alertmanagerURL":       {},
	"authProxyIP":           {},
	"authProxyPassword":     {},
	"deploymentOverrides":   {},
	"gitoliteIP":            {},
	"gitserverCount":        {},
	"gitserverDiskSize":     {},
	"gitserverSSH":          {},
	"httpNodePort":          {},
	"httpsNodePort":         {},
	"indexedSearchDiskSize": {},
	"langGo":                {},
	"langJava":              {},
	"langJavaScript":        {},
	"langPHP":               {},
	"langPython":            {},
	"langSwift":             {},
	"langTypeScript":        {},
	"namespace":             {},
	"nodeSSDPath":           {},
	"phabricatorIP":         {},
	"prometheus":            {},
	"pyPIIP":                {},
	"rbac":                  {},
	"storageClass":          {},
	"useAlertManager":       {},
}

const redactedSecret = "REDACTED"

// problemKind represents the kind of a configuration problem.
type problemKind string

const (
	problemSite            problemKind = "SiteConfig"
	problemExternalService problemKind = "ExternalService"
)

// Problem contains kind and description of a specific configuration problem.
type Problem struct {
	kind        problemKind
	description string
}

// NewSiteProblem creates a new site config problem with given message.
func NewSiteProblem(msg string) *Problem {
	return &Problem{
		kind:        problemSite,
		description: msg,
	}
}

// IsSite returns true if the problem is about site config.
func (p Problem) IsSite() bool {
	return p.kind == problemSite
}

// IsExternalService returns true if the problem is about external service config.
func (p Problem) IsExternalService() bool {
	return p.kind == problemExternalService
}

func (p Problem) String() string {
	return p.description
}

func (p *Problem) MarshalJSON() ([]byte, error) {
	return json.Marshal(map[string]string{
		"kind":        string(p.kind),
		"description": p.description,
	})
}

func (p *Problem) UnmarshalJSON(b []byte) error {
	var m map[string]string
	if err := json.Unmarshal(b, &m); err != nil {
		return err
	}
	p.kind = problemKind(m["kind"])
	p.description = m["description"]
	return nil
}

// Problems is a list of problems.
type Problems []*Problem

// newProblems converts a list of messages with their kind into problems.
func newProblems(kind problemKind, messages ...string) Problems {
	problems := make([]*Problem, len(messages))
	for i := range messages {
		problems[i] = &Problem{
			kind:        kind,
			description: messages[i],
		}
	}
	return problems
}

// NewSiteProblems converts a list of messages into site config problems.
func NewSiteProblems(messages ...string) Problems {
	return newProblems(problemSite, messages...)
}

// NewExternalServiceProblems converts a list of messages into external service config problems.
func NewExternalServiceProblems(messages ...string) Problems {
	return newProblems(problemExternalService, messages...)
}

// Messages returns the list of problems in strings.
func (ps Problems) Messages() []string {
	if len(ps) == 0 {
		return nil
	}

	msgs := make([]string, len(ps))
	for i := range ps {
		msgs[i] = ps[i].String()
	}
	return msgs
}

// Site returns all site config problems in the list.
func (ps Problems) Site() (problems Problems) {
	for i := range ps {
		if ps[i].IsSite() {
			problems = append(problems, ps[i])
		}
	}
	return problems
}

// ExternalService returns all external service config problems in the list.
func (ps Problems) ExternalService() (problems Problems) {
	for i := range ps {
		if ps[i].IsExternalService() {
			problems = append(problems, ps[i])
		}
	}
	return problems
}

// Validate validates the configuration against the JSON Schema and other
// custom validation checks.
func Validate(input conftypes.RawUnified) (problems Problems, err error) {
	siteJSON, err := jsonc.Parse(input.Site)
	if err != nil {
		return nil, err
	}

	siteProblems := doValidate(siteJSON, schema.SiteSchemaJSON)
	problems = append(problems, NewSiteProblems(siteProblems...)...)

	customProblems, err := validateCustomRaw(conftypes.RawUnified{
		Site: string(siteJSON),
	})
	if err != nil {
		return nil, err
	}
	problems = append(problems, customProblems...)
	return problems, nil
}

// ValidateSite is like Validate, except it only validates the site configuration.
func ValidateSite(input string) (messages []string, err error) {
	raw := Raw()
	unredacted, err := UnredactSecrets(input, raw)
	if err != nil {
		return nil, err
	}
	raw.Site = unredacted

	problems, err := Validate(raw)
	if err != nil {
		return nil, err
	}
	return problems.Messages(), nil
}

// siteConfigSecrets is the list of secrets in site config needs to be redacted
// before serving or unredacted before saving.
var siteConfigSecrets = []struct {
	readPath  string // gjson uses "." as path separator, uses "\" to escape.
	editPaths []string
}{
	{readPath: `executors\.accessToken`, editPaths: []string{"executors.accessToken"}},
	{readPath: `email\.smtp.username`, editPaths: []string{"email.smtp", "username"}},
	{readPath: `email\.smtp.password`, editPaths: []string{"email.smtp", "password"}},
	{readPath: `organizationInvitations.signingKey`, editPaths: []string{"organizationInvitations", "signingKey"}},
	{readPath: `githubClientSecret`, editPaths: []string{"githubClientSecret"}},
	{readPath: `dotcom.githubApp\.cloud.clientSecret`, editPaths: []string{"dotcom", "githubApp.cloud", "clientSecret"}},
	{readPath: `dotcom.githubApp\.cloud.privateKey`, editPaths: []string{"dotcom", "githubApp.cloud", "privateKey"}},
	{readPath: `gitHubApp.privateKey`, editPaths: []string{"gitHubApp", "privateKey"}},
	{readPath: `gitHubApp.clientSecret`, editPaths: []string{"gitHubApp", "clientSecret"}},
	{readPath: `auth\.unlockAccountLinkSigningKey`, editPaths: []string{"auth.unlockAccountLinkSigningKey"}},
	{readPath: `dotcom.srcCliVersionCache.github.token`, editPaths: []string{"dotcom", "srcCliVersionCache", "github", "token"}},
	{readPath: `dotcom.srcCliVersionCache.github.webhookSecret`, editPaths: []string{"dotcom", "srcCliVersionCache", "github", "webhookSecret"}},
	{readPath: `embeddings.accessToken`, editPaths: []string{"embeddings", "accessToken"}},
	{readPath: `completions.accessToken`, editPaths: []string{"completions", "accessToken"}},
	{readPath: `app.dotcomAuthToken`, editPaths: []string{"app", "dotcomAuthToken"}},
}

// UnredactSecrets unredacts unchanged secrets back to their original value for
// the given configuration.
//
// Updates to this function should also be reflected in the RedactSecrets.
func UnredactSecrets(input string, raw conftypes.RawUnified) (string, error) {
	oldCfg, err := ParseConfig(raw)
	if err != nil {
		return input, errors.Wrap(err, "parse old config")
	}

	oldSecrets := make(map[string]string, len(oldCfg.AuthProviders))
	for _, ap := range oldCfg.AuthProviders {
		if ap.Openidconnect != nil {
			oldSecrets[ap.Openidconnect.ClientID] = ap.Openidconnect.ClientSecret
		}
		if ap.Github != nil {
			oldSecrets[ap.Github.ClientID] = ap.Github.ClientSecret
		}
		if ap.Gitlab != nil {
			oldSecrets[ap.Gitlab.ClientID] = ap.Gitlab.ClientSecret
		}
		if ap.Bitbucketcloud != nil {
			oldSecrets[ap.Bitbucketcloud.ClientKey] = ap.Bitbucketcloud.ClientSecret
		}
		if ap.AzureDevOps != nil {
			oldSecrets[ap.AzureDevOps.ClientID] = ap.AzureDevOps.ClientSecret
		}
	}

	newCfg, err := ParseConfig(conftypes.RawUnified{
		Site: input,
	})
	if err != nil {
		return input, errors.Wrap(err, "parse new config")
	}
	for _, ap := range newCfg.AuthProviders {
		if ap.Openidconnect != nil && ap.Openidconnect.ClientSecret == redactedSecret {
			ap.Openidconnect.ClientSecret = oldSecrets[ap.Openidconnect.ClientID]
		}
		if ap.Github != nil && ap.Github.ClientSecret == redactedSecret {
			ap.Github.ClientSecret = oldSecrets[ap.Github.ClientID]
		}
		if ap.Gitlab != nil && ap.Gitlab.ClientSecret == redactedSecret {
			ap.Gitlab.ClientSecret = oldSecrets[ap.Gitlab.ClientID]
		}
		if ap.Bitbucketcloud != nil && ap.Bitbucketcloud.ClientSecret == redactedSecret {
			ap.Bitbucketcloud.ClientSecret = oldSecrets[ap.Bitbucketcloud.ClientKey]
		}
		if ap.AzureDevOps != nil && ap.AzureDevOps.ClientSecret == redactedSecret {
			ap.AzureDevOps.ClientSecret = oldSecrets[ap.AzureDevOps.ClientID]
		}
	}
	unredactedSite, err := jsonc.Edit(input, newCfg.AuthProviders, "auth.providers")
	if err != nil {
		return input, errors.Wrap(err, `unredact "auth.providers"`)
	}

	for _, secret := range siteConfigSecrets {
		v := gjson.Get(unredactedSite, secret.readPath).String()
		if v != redactedSecret {
			continue
		}

		unredactedSite, err = jsonc.Edit(unredactedSite, gjson.Get(raw.Site, secret.readPath).String(), secret.editPaths...)
		if err != nil {
			return input, errors.Wrapf(err, `unredact %q`, strings.Join(secret.editPaths, " > "))
		}
	}

	formattedSite, err := jsonc.Format(unredactedSite, &jsonc.DefaultFormatOptions)
	if err != nil {
		return input, errors.Wrapf(err, "JSON formatting")
	}

	return formattedSite, err
}

func RedactSecrets(raw conftypes.RawUnified) (empty conftypes.RawUnified, err error) {
	return redactConfSecrets(raw, false)
}

func RedactAndHashSecrets(raw conftypes.RawUnified) (empty conftypes.RawUnified, err error) {
	return redactConfSecrets(raw, true)
}

// redactConfSecrets redacts defined list of secrets from the given configuration. It returns empty
// configuration if any error occurs during redacting process to prevent accidental leak of secrets
// in the configuration.
//
// Updates to this function should also be reflected in the UnredactSecrets.
func redactConfSecrets(raw conftypes.RawUnified, hashSecrets bool) (empty conftypes.RawUnified, err error) {
	getRedactedSecret := func(_ string) string { return redactedSecret }
	if hashSecrets {
		getRedactedSecret = func(secret string) string {
			hash := hashutil.ToSHA256Bytes([]byte(secret))
			digest := hex.EncodeToString(hash)
			return "REDACTED-DATA-CHUNK" + "-" + digest[:10]
		}
	}

	cfg, err := ParseConfig(raw)
	if err != nil {
		return empty, errors.Wrap(err, "parse config")
	}

	for _, ap := range cfg.AuthProviders {
		if ap.Openidconnect != nil {
			ap.Openidconnect.ClientSecret = getRedactedSecret(ap.Openidconnect.ClientSecret)
		}
		if ap.Github != nil {
			ap.Github.ClientSecret = getRedactedSecret(ap.Github.ClientSecret)
		}
		if ap.Gitlab != nil {
			ap.Gitlab.ClientSecret = getRedactedSecret(ap.Gitlab.ClientSecret)
		}
		if ap.Bitbucketcloud != nil {
			ap.Bitbucketcloud.ClientSecret = getRedactedSecret(ap.Bitbucketcloud.ClientSecret)
		}
		if ap.AzureDevOps != nil {
			ap.AzureDevOps.ClientSecret = getRedactedSecret(ap.AzureDevOps.ClientSecret)
		}
	}
	redactedSite := raw.Site
	if len(cfg.AuthProviders) > 0 {
		redactedSite, err = jsonc.Edit(raw.Site, cfg.AuthProviders, "auth.providers")
		if err != nil {
			return empty, errors.Wrap(err, `redact "auth.providers"`)
		}
	}

	for _, secret := range siteConfigSecrets {
		val := gjson.Get(redactedSite, secret.readPath).String()
		if val == "" {
			continue
		}

		redactedSite, err = jsonc.Edit(redactedSite, getRedactedSecret(val), secret.editPaths...)
		if err != nil {
			return empty, errors.Wrapf(err, `redact %q`, strings.Join(secret.editPaths, " > "))
		}
	}

	formattedSite, err := jsonc.Format(redactedSite, &jsonc.DefaultFormatOptions)
	if err != nil {
		return empty, errors.Wrapf(err, "JSON formatting")
	}

	return conftypes.RawUnified{
		Site: formattedSite,
	}, err
}

// ValidateSettings validates the JSONC input against the settings JSON Schema, returning a list of
// problems (if any).
func ValidateSettings(jsoncInput string) (problems []string) {
	jsonInput, err := jsonc.Parse(jsoncInput)
	if err != nil {
		return []string{err.Error()}
	}

	return doValidate(jsonInput, schema.SettingsSchemaJSON)
}

func doValidate(input []byte, schema string) (messages []string) {
	res, err := validate([]byte(schema), input)
	if err != nil {
		// We can't return more detailed problems because the input completely failed to parse, so
		// just return the parse error as the problem.
		return []string{err.Error()}
	}
	messages = make([]string, 0, len(res.Errors()))
	for _, e := range res.Errors() {
		if _, ok := ignoreLegacyKubernetesFields[e.Field()]; ok {
			continue
		}

		var keyPath string
		if c := e.Context(); c != nil {
			keyPath = strings.TrimPrefix(e.Context().String("."), "(root).")
		} else {
			keyPath = e.Field()
		}

		// Use an easier-to-understand description for the common case when the root is not an
		// object (which can happen when the input is derived from JSONC that is entirely commented
		// out, for example).
		if e, ok := e.(*gojsonschema.InvalidTypeError); ok && e.Field() == "(root)" && strings.HasPrefix(e.Description(), "Invalid type. Expected: object, given: ") {
			messages = append(messages, "must be a JSON object (use {} for empty)")
			continue
		}

		messages = append(messages, fmt.Sprintf("%s: %s", keyPath, e.Description()))
	}
	return messages
}

func validate(schema, input []byte) (*gojsonschema.Result, error) {
	s, err := gojsonschema.NewSchema(jsonLoader{gojsonschema.NewBytesLoader(schema)})
	if err != nil {
		return nil, err
	}
	return s.Validate(gojsonschema.NewBytesLoader(input))
}

type jsonLoader struct {
	gojsonschema.JSONLoader
}

func (l jsonLoader) LoaderFactory() gojsonschema.JSONLoaderFactory {
	return &jsonLoaderFactory{}
}

type jsonLoaderFactory struct{}

func (f jsonLoaderFactory) New(source string) gojsonschema.JSONLoader {
	switch source {
	case "settings.schema.json":
		return gojsonschema.NewStringLoader(schema.SettingsSchemaJSON)
	case "site.schema.json":
		return gojsonschema.NewStringLoader(schema.SiteSchemaJSON)
	}
	return nil
}

// MustValidateDefaults should be called after all custom validators have been
// registered. It will panic if any of the default deployment configurations
// are invalid.
func MustValidateDefaults() {
	mustValidate("DevAndTesting", confdefaults.DevAndTesting)
	mustValidate("DockerContainer", confdefaults.DockerContainer)
	mustValidate("KubernetesOrDockerComposeOrPureDocker", confdefaults.KubernetesOrDockerComposeOrPureDocker)
}

// mustValidate panics if the configuration does not pass validation.
func mustValidate(name string, cfg conftypes.RawUnified) {
	problems, err := Validate(cfg)
	if err != nil {
		panic(fmt.Sprintf("Error with %q: %s", name, err))
	}
	if len(problems) > 0 {
		panic(fmt.Sprintf("conf: problems with default configuration for %q:\n  %s", name, strings.Join(problems.Messages(), "\n  ")))
	}
}
