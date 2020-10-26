package apiworker

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/apiworker/apiclient"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/apiworker/command"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

type Options struct {
	// QueueName is the name of the queue to process work from. Having this configurable
	// allows us to have multiple worker pools with different resource requirements and
	// horizontal scaling factors while still uniformly processing events.
	QueueName string

	// HeartbeatInterval denotes the time between heartbeat requests to the queue API.
	HeartbeatInterval time.Duration

	// GitServicePath is the path to the internal git service API proxy in the frontend.
	// This path should contain the endpoints info/refs and git-upload-pack.
	GitServicePath string

	// RedactedValues is a list of values to scrub from the command output before sending
	// it to the underlying job store. This should contain all environment variables which
	// may be sensitive to being visible (e.g. shared frontend API passwords).
	RedactedValues []string

	// WorkerOptions configures the worker behavior.
	WorkerOptions workerutil.WorkerOptions

	// ClientOptions configures the client that interacts with the queue API.
	ClientOptions apiclient.Options

	// FirecrackerOptions configures the behavior of Firecracker virtual machine creation.
	FirecrackerOptions command.FirecrackerOptions

	// ResourceOptions configures the resource limits of docker container and Firecracker
	// virtual machines running on the executor.
	ResourceOptions command.ResourceOptions
}

// NewWorker creates a worker that polls a remote job queue API for work. The returned
// routine contains both a worker that periodically polls for new work to perform, as well
// as a heartbeat routine that will periodically hit the remote API with the work that is
// currently being performed, which is necessary so the job queue API doesn't hand out jobs
// it thinks may have been dropped.
func NewWorker(options Options) goroutine.BackgroundRoutine {
	idSet := newIDSet()
	queueStore := apiclient.New(options.ClientOptions)
	store := &storeShim{queueName: options.QueueName, queueStore: queueStore}
	handler := &handler{idSet: idSet, options: options, runnerFactory: command.NewRunner}
	indexer := workerutil.NewWorker(context.Background(), store, handler, options.WorkerOptions)
	heartbeat := goroutine.NewHandlerWithErrorMessage("heartbeat", func(ctx context.Context) error {
		return queueStore.Heartbeat(ctx, idSet.Slice())
	})

	return goroutine.CombinedRoutine{
		indexer,
		goroutine.NewPeriodicGoroutine(context.Background(), options.HeartbeatInterval, heartbeat),
	}
}
