package conf

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
	"github.com/xeipuuv/gojsonschema"
)

// ignoreLegacyDataCenterFields is the set of field names for which validation errors
// should be ignored. The validation errors occur only because Data Center config merged
// site config and Data Center-specific config. This is deprecated. Until we have
// transitioned fully, we suppress validation errors on these fields.
var ignoreLegacyDataCenterFields = map[string]struct{}{
	// To get a list of all keys: jq '.properties | keys' < datacenter.schema.json
	"alertmanagerConfig":    struct{}{},
	"alertmanagerURL":       struct{}{},
	"authProxyIP":           struct{}{},
	"authProxyPassword":     struct{}{},
	"deploymentOverrides":   struct{}{},
	"gitoliteIP":            struct{}{},
	"gitserverCount":        struct{}{},
	"gitserverDiskSize":     struct{}{},
	"gitserverSSH":          struct{}{},
	"httpNodePort":          struct{}{},
	"httpsNodePort":         struct{}{},
	"indexedSearchDiskSize": struct{}{},
	"langGo":                struct{}{},
	"langJava":              struct{}{},
	"langJavaScript":        struct{}{},
	"langPHP":               struct{}{},
	"langPython":            struct{}{},
	"langSwift":             struct{}{},
	"langTypeScript":        struct{}{},
	"namespace":             struct{}{},
	"nodeSSDPath":           struct{}{},
	"phabricatorIP":         struct{}{},
	"prometheus":            struct{}{},
	"pyPIIP":                struct{}{},
	"rbac":                  struct{}{},
	"storageClass":          struct{}{},
	"useAlertManager":       struct{}{},
}

// Validate validates the site configuration the JSON Schema and other custom validation
// checks.
func Validate(inputStr string) (problems []string, err error) {
	input := []byte(jsonc.Normalize(inputStr))

	input, _, err = expandEnv(input)
	if err != nil {
		return nil, err
	}

	res, err := validate([]byte(schema.SiteSchemaJSON), input)
	if err != nil {
		return nil, err
	}
	problems = make([]string, 0, len(res.Errors()))
	for _, e := range res.Errors() {
		if _, ok := ignoreLegacyDataCenterFields[e.Field()]; ok {
			continue
		}

		var keyPath string
		if c := e.Context(); c != nil {
			keyPath = strings.TrimPrefix(e.Context().String("."), "(root).")
		} else {
			keyPath = e.Field()
		}

		// TEMPORARY: Ignore validation errors in the singleton auth config because we can
		// 100% infer them for now.
		//
		// TODO(sqs): Remove this. https://github.com/sourcegraph/sourcegraph/issues/11148
		if e.Field() == "type" && (keyPath == "auth.openIDConnect" || keyPath == "auth.saml") {
			continue
		}
		if e.Field() == "auth.saml.type" || e.Field() == "auth.openIDConnect.type" {
			continue
		}

		if !MultipleAuthProvidersEnabled() && keyPath == "(root)" && e.Description() == "Must validate \"else\" as \"if\" was not valid" {
			continue
		}

		problems = append(problems, fmt.Sprintf("%s: %s", keyPath, e.Description()))
	}

	problems2, err := validateCustomRaw(input)
	if err != nil {
		return nil, err
	}
	problems = append(problems, problems2...)

	return problems, nil
}

func validate(schema, input []byte) (*gojsonschema.Result, error) {
	if len(input) > 0 {
		// HACK: Remove the "settings" field from site config because
		// github.com/xeipuuv/gojsonschema has a bug where $ref'd schemas do not always get
		// loaded. When https://github.com/xeipuuv/gojsonschema/pull/196 is merged, it will probably
		// be fixed. This means that the backend config validation will not validate settings, but
		// that is OK because specifying settings here is discouraged anyway.
		var v map[string]interface{}
		if err := json.Unmarshal(input, &v); err != nil {
			return nil, err
		}
		delete(v, "settings")
		var err error
		input, err = json.Marshal(v)
		if err != nil {
			return nil, err
		}
	}

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
	case "https://sourcegraph.com/v1/settings.schema.json":
		return gojsonschema.NewStringLoader(schema.SettingsSchemaJSON)
	case "https://sourcegraph.com/v1/site.schema.json":
		return gojsonschema.NewStringLoader(schema.SiteSchemaJSON)
	}
	return nil
}
