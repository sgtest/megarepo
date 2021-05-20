package upload

import (
	"time"

	"github.com/sourcegraph/sourcegraph/lib/output"
)

type UploadOptions struct {
	SourcegraphInstanceOptions
	OutputOptions
	UploadRecordOptions
}

type SourcegraphInstanceOptions struct {
	SourcegraphURL      string            // The URL (including scheme) of the target Sourcegraph instance
	AccessToken         string            // The user access token
	AdditionalHeaders   map[string]string // Additional request headers on each request
	Path                string            // Custom path on the Sourcegraph instance (used internally)
	GitHubToken         string            // GitHub token used for auth when lsif.enforceAuth is true (optional)
	MaxRetries          int               // The maximum number of retries per request
	RetryInterval       time.Duration     // Sleep duration between retries
	MaxPayloadSizeBytes int64             // The maximum number of bytes sent in a single request
}

type OutputOptions struct {
	Logger RequestLogger  // Logger of all HTTP request/responses (optional)
	Output *output.Output // Output instance used for fancy output (optional)
}

type UploadRecordOptions struct {
	Repo              string
	Commit            string
	Root              string
	Indexer           string
	AssociatedIndexID *int
}
