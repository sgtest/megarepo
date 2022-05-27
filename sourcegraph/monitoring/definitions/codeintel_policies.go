package definitions

import (
	"github.com/sourcegraph/sourcegraph/monitoring/definitions/shared"
	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

func CodeIntelPolicies() *monitoring.Container {
	return &monitoring.Container{
		Name:        "codeintel-policies",
		Title:       "Code Intelligence > Policies",
		Description: "The service at `internal/codeintel/policies`.",
		Variables:   []monitoring.ContainerVariable{},
		Groups: []monitoring.Group{
			shared.CodeIntelligence.NewRepoMatcherTaskGroup(""),
		},
	}
}
