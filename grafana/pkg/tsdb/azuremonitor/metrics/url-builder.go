package metrics

import (
	"fmt"
	"strings"
)

// urlBuilder builds the URL for calling the Azure Monitor API
type urlBuilder struct {
	ResourceURI *string

	// Following fields will be used to generate a ResourceURI
	DefaultSubscription *string
	Subscription        *string
	ResourceGroup       *string
	MetricNamespace     *string
	ResourceName        *string
	MetricDefinition    *string
}

func (params *urlBuilder) buildResourceURI() (*string, error) {
	if params.ResourceURI != nil && *params.ResourceURI != "" {
		return params.ResourceURI, nil
	}

	subscription := params.Subscription

	if params.Subscription == nil || *params.Subscription == "" {
		subscription = params.DefaultSubscription
	}

	metricNamespace := params.MetricNamespace

	if metricNamespace == nil || *metricNamespace == "" {
		if params.MetricDefinition == nil || *params.MetricDefinition == "" {
			return nil, fmt.Errorf("no metricNamespace or metricDefiniton value provided")
		}
		metricNamespace = params.MetricDefinition
	}

	metricNamespaceArray := strings.Split(*metricNamespace, "/")
	var resourceNameArray []string
	if params.ResourceName != nil && *params.ResourceName != "" {
		resourceNameArray = strings.Split(*params.ResourceName, "/")
	}
	provider := metricNamespaceArray[0]
	metricNamespaceArray = metricNamespaceArray[1:]

	if strings.HasPrefix(strings.ToLower(*metricNamespace), "microsoft.storage/storageaccounts/") &&
		params.ResourceName != nil &&
		!strings.HasSuffix(*params.ResourceName, "default") {
		resourceNameArray = append(resourceNameArray, "default")
	}

	resGroup := ""
	if params.ResourceGroup != nil {
		resGroup = *params.ResourceGroup
	}
	urlArray := []string{
		"/subscriptions",
		*subscription,
		"resourceGroups",
		resGroup,
		"providers",
		provider,
	}

	for i, namespace := range metricNamespaceArray {
		urlArray = append(urlArray, namespace, resourceNameArray[i])
	}

	resourceURI := strings.Join(urlArray, "/")
	return &resourceURI, nil
}

// BuildSubscriptionMetricsURL returns a URL for querying metrics for all resources in a subscription
// It requires to set a $filter and a region parameter
func BuildSubscriptionMetricsURL(subscription string) string {
	return fmt.Sprintf("/subscriptions/%s/providers/microsoft.insights/metrics", subscription)
}
