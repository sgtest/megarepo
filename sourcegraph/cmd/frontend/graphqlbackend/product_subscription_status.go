package graphqlbackend

import (
	"context"
)

// GetProductNameWithBrand is called to obtain the full product name (e.g., "Sourcegraph OSS") from a
// product license.
var GetProductNameWithBrand = func(hasLicense bool, licenseTags []string) string {
	return "Sourcegraph OSS"
}

// ActualUserCount is called to obtain the actual maximum number of user accounts that have been active
// on this Sourcegraph instance for the current license.
var ActualUserCount = func(ctx context.Context) (int32, error) {
	return 0, nil
}

// productSubscriptionStatus implements the GraphQL type ProductSubscriptionStatus.
type productSubscriptionStatus struct{}

func (productSubscriptionStatus) ProductNameWithBrand() (string, error) {
	info, err := GetConfiguredProductLicenseInfo()
	if err != nil {
		return "", err
	}
	hasLicense := info != nil
	var licenseTags []string
	if hasLicense {
		licenseTags = info.Tags()
	}
	return GetProductNameWithBrand(hasLicense, licenseTags), nil
}

func (productSubscriptionStatus) ActualUserCount(ctx context.Context) (int32, error) {
	return ActualUserCount(ctx)
}

func (r productSubscriptionStatus) License() (*ProductLicenseInfo, error) {
	return GetConfiguredProductLicenseInfo()
}
