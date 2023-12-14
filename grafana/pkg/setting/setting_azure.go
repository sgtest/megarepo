package setting

import (
	"github.com/grafana/grafana-azure-sdk-go/azsettings"
	"github.com/grafana/grafana/pkg/util"
)

func (cfg *Cfg) readAzureSettings() {
	azureSettings := &azsettings.AzureSettings{}

	azureSection := cfg.Raw.Section("azure")

	// Cloud
	cloudName := azureSection.Key("cloud").MustString(azsettings.AzurePublic)
	azureSettings.Cloud = azsettings.NormalizeAzureCloud(cloudName)

	// Managed Identity authentication
	azureSettings.ManagedIdentityEnabled = azureSection.Key("managed_identity_enabled").MustBool(false)
	azureSettings.ManagedIdentityClientId = azureSection.Key("managed_identity_client_id").String()

	// Workload Identity authentication
	if azureSection.Key("workload_identity_enabled").MustBool(false) {
		azureSettings.WorkloadIdentityEnabled = true
		workloadIdentitySettings := &azsettings.WorkloadIdentitySettings{}

		if val := azureSection.Key("workload_identity_tenant_id").String(); val != "" {
			workloadIdentitySettings.TenantId = val
		}
		if val := azureSection.Key("workload_identity_client_id").String(); val != "" {
			workloadIdentitySettings.ClientId = val
		}
		if val := azureSection.Key("workload_identity_token_file").String(); val != "" {
			workloadIdentitySettings.TokenFile = val
		}

		azureSettings.WorkloadIdentitySettings = workloadIdentitySettings
	}

	// User Identity authentication
	if azureSection.Key("user_identity_enabled").MustBool(false) {
		azureSettings.UserIdentityEnabled = true
		tokenEndpointSettings := &azsettings.TokenEndpointSettings{}

		// Get token endpoint from Azure AD settings if enabled
		azureAdSection := cfg.Raw.Section("auth.azuread")
		if azureAdSection.Key("enabled").MustBool(false) {
			tokenEndpointSettings.TokenUrl = azureAdSection.Key("token_url").String()
			tokenEndpointSettings.ClientId = azureAdSection.Key("client_id").String()
			tokenEndpointSettings.ClientSecret = azureAdSection.Key("client_secret").String()
		}

		// Override individual settings
		if val := azureSection.Key("user_identity_token_url").String(); val != "" {
			tokenEndpointSettings.TokenUrl = val
		}
		if val := azureSection.Key("user_identity_client_id").String(); val != "" {
			tokenEndpointSettings.ClientId = val
			tokenEndpointSettings.ClientSecret = ""
		}
		if val := azureSection.Key("user_identity_client_secret").String(); val != "" {
			tokenEndpointSettings.ClientSecret = val
		}

		azureSettings.UserIdentityTokenEndpoint = tokenEndpointSettings
	}

	azureSettings.ForwardSettingsPlugins = util.SplitString(azureSection.Key("forward_settings_to_plugins").String())

	cfg.Azure = azureSettings
}
