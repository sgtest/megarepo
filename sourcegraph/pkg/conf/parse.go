package conf

import (
	"encoding/json"

	"github.com/sourcegraph/sourcegraph/pkg/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
)

// parseConfigData parses the provided config string into the given cfg struct
// pointer.
func parseConfigData(data string, cfg interface{}) error {
	if data != "" {
		data, err := jsonc.Parse(data)
		if err != nil {
			return err
		}
		if err := json.Unmarshal(data, cfg); err != nil {
			return err
		}
	}

	if v, ok := cfg.(*schema.SiteConfiguration); ok {
		// For convenience, make sure this is not nil.
		if v.ExperimentalFeatures == nil {
			v.ExperimentalFeatures = &schema.ExperimentalFeatures{}
		}
	}
	return nil
}

// ParseConfig parses the raw configuration.
func ParseConfig(data conftypes.RawUnified) (*Unified, error) {
	cfg := &Unified{
		ServiceConnections: data.ServiceConnections,
	}
	if err := parseConfigData(data.Critical, &cfg.Critical); err != nil {
		return nil, err
	}
	if err := parseConfigData(data.Site, &cfg.SiteConfiguration); err != nil {
		return nil, err
	}
	return cfg, nil
}

// requireRestart describes the list of config properties that require
// restarting the Sourcegraph Server in order for the change to take effect.
//
// Experimental features are special in that they are denoted individually
// via e.g. "experimentalFeatures::myFeatureFlag".
var requireRestart = []string{
	"auth.accessTokens",
	"auth.sessionExpiry",
	"auth.disableAccessTokens",
	"git.cloneURLToRepositoryName",
	"searchScopes",
	"extensions",
	"disableBrowserExtension",
	"disablePublicRepoRedirects",

	// Options defined in critical.schema.json are prefixed with "critical::"
	"critical::lightstepAccessToken",
	"critical::lightstepProject",
	"critical::auth.userOrgMap",
	"critical::auth.providers",
	"critical::externalURL",
	"critical::tls.letsencrypt",
	"critical::tlsCert",
	"critical::tlsKey",
	"critical::update.channel",
	"critical::useJaeger",
}

// NeedRestartToApply determines if a restart is needed to apply the changes
// between the two configurations.
func NeedRestartToApply(before, after *Unified) bool {
	// Check every option that changed to determine whether or not a server
	// restart is required.
	for option := range diff(before, after) {
		for _, requireRestartOption := range requireRestart {
			if option == requireRestartOption {
				return true
			}
		}
	}
	return false
}
