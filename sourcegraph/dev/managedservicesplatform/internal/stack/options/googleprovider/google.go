package googleprovider

import (
	google "github.com/sourcegraph/managed-services-platform-cdktf/gen/google/provider"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

// With modifies a new stack to use the Google Terraform provider
// with the given project ID.
//
// All GCP resources created under a stack with this option should still explicitly
// configure ProjectID individually.
func With(projectID string) stack.NewStackOption {
	return func(s stack.Stack) {
		var project *string
		if projectID != "" {
			project = pointers.Ptr(projectID)
		}
		_ = google.NewGoogleProvider(s.Stack, pointers.Ptr("google"), &google.GoogleProviderConfig{
			Project: project,
		})
	}
}
