package azuremonitor

import (
	"fmt"

	"github.com/grafana/grafana-azure-sdk-go/azcredentials"
	"github.com/grafana/grafana-azure-sdk-go/azsettings"

	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/tsdb/azuremonitor/types"
)

// Azure cloud names specific to Azure Monitor
const (
	azureMonitorPublic       = "azuremonitor"
	azureMonitorChina        = "chinaazuremonitor"
	azureMonitorUSGovernment = "govazuremonitor"
	azureMonitorCustomized   = "customizedazuremonitor"
)

func getAuthType(cfg *setting.Cfg, jsonData *types.AzureClientSettings) string {
	if azureAuthType := jsonData.AzureAuthType; azureAuthType != "" {
		return azureAuthType
	} else {
		tenantId := jsonData.TenantId
		clientId := jsonData.ClientId

		// If authentication type isn't explicitly specified and datasource has client credentials,
		// then this is existing datasource which is configured for app registration (client secret)
		if tenantId != "" && clientId != "" {
			return azcredentials.AzureAuthClientSecret
		}

		// For newly created datasource with no configuration the order is as follows:
		// Managed identity is the default if enabled
		// Workload identity is the next option if enabled
		// Client secret is the final fallback
		if cfg.Azure.ManagedIdentityEnabled {
			return azcredentials.AzureAuthManagedIdentity
		} else if cfg.Azure.WorkloadIdentityEnabled {
			return azcredentials.AzureAuthWorkloadIdentity
		} else {
			return azcredentials.AzureAuthClientSecret
		}
	}
}

func getDefaultAzureCloud(cfg *setting.Cfg) (string, error) {
	// Allow only known cloud names
	cloudName := ""
	if cfg != nil && cfg.Azure != nil {
		cloudName = cfg.Azure.Cloud
	}
	switch cloudName {
	case azsettings.AzurePublic:
		return azsettings.AzurePublic, nil
	case azsettings.AzureChina:
		return azsettings.AzureChina, nil
	case azsettings.AzureUSGovernment:
		return azsettings.AzureUSGovernment, nil
	case azsettings.AzureCustomized:
		return azsettings.AzureCustomized, nil
	case "":
		// Not set cloud defaults to public
		return azsettings.AzurePublic, nil
	default:
		err := fmt.Errorf("the cloud '%s' not supported", cloudName)
		return "", err
	}
}

func normalizeAzureCloud(cloudName string) (string, error) {
	switch cloudName {
	case azureMonitorPublic:
		return azsettings.AzurePublic, nil
	case azureMonitorChina:
		return azsettings.AzureChina, nil
	case azureMonitorUSGovernment:
		return azsettings.AzureUSGovernment, nil
	case azureMonitorCustomized:
		return azsettings.AzureCustomized, nil
	default:
		err := fmt.Errorf("the cloud '%s' not supported", cloudName)
		return "", err
	}
}

func getAzureCloud(cfg *setting.Cfg, jsonData *types.AzureClientSettings) (string, error) {
	authType := getAuthType(cfg, jsonData)
	switch authType {
	case azcredentials.AzureAuthManagedIdentity, azcredentials.AzureAuthWorkloadIdentity:
		// In case of managed identity and workload identity, the cloud is always same as where Grafana is hosted
		return getDefaultAzureCloud(cfg)
	case azcredentials.AzureAuthClientSecret:
		if cloud := jsonData.CloudName; cloud != "" {
			return normalizeAzureCloud(cloud)
		} else {
			return getDefaultAzureCloud(cfg)
		}
	default:
		err := fmt.Errorf("the authentication type '%s' not supported", authType)
		return "", err
	}
}

func getAzureCredentials(cfg *setting.Cfg, jsonData *types.AzureClientSettings, secureJsonData map[string]string) (azcredentials.AzureCredentials, error) {
	authType := getAuthType(cfg, jsonData)

	switch authType {
	case azcredentials.AzureAuthManagedIdentity:
		credentials := &azcredentials.AzureManagedIdentityCredentials{}
		return credentials, nil
	case azcredentials.AzureAuthWorkloadIdentity:
		credentials := &azcredentials.AzureWorkloadIdentityCredentials{}
		return credentials, nil
	case azcredentials.AzureAuthClientSecret:
		cloud, err := getAzureCloud(cfg, jsonData)
		if err != nil {
			return nil, err
		}
		if secureJsonData["clientSecret"] == "" {
			return nil, fmt.Errorf("unable to instantiate credentials, clientSecret must be set")
		}
		credentials := &azcredentials.AzureClientSecretCredentials{
			AzureCloud:   cloud,
			TenantId:     jsonData.TenantId,
			ClientId:     jsonData.ClientId,
			ClientSecret: secureJsonData["clientSecret"],
		}
		return credentials, nil

	default:
		err := fmt.Errorf("the authentication type '%s' not supported", authType)
		return nil, err
	}
}
