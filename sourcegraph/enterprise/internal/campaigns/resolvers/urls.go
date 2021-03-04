package resolvers

import (
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
)

func campaignsApplyURL(n graphqlbackend.Namespace, c graphqlbackend.BatchSpecResolver) string {
	return n.URL() + "/campaigns/apply/" + string(c.ID())
}

func campaignURL(n graphqlbackend.Namespace, c graphqlbackend.BatchChangeResolver) string {
	// This needs to be kept consistent with campaigns.campaignURL().
	return n.URL() + "/campaigns/" + c.Name()
}
