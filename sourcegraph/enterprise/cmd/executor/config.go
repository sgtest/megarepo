package main

import (
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/apiworker"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/apiworker/apiclient"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/apiworker/command"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

type Config struct {
	env.BaseConfig

	FrontendURL          string
	FrontendUsername     string
	FrontendPassword     string
	QueueName            string
	QueuePollInterval    time.Duration
	HeartbeatInterval    time.Duration
	MaximumNumJobs       int
	FirecrackerImage     string
	UseFirecracker       bool
	FirecrackerNumCPUs   int
	FirecrackerMemory    string
	FirecrackerDiskSpace string
	ImageArchivesPath    string
	DisableHealthServer  bool
}

func (c *Config) Load() {
	c.FrontendURL = c.Get("EXECUTOR_FRONTEND_URL", "", "The external URL of the sourcegraph instance.")
	c.FrontendUsername = c.Get("EXECUTOR_FRONTEND_USERNAME", "", "The username supplied to the frontend.")
	c.FrontendPassword = c.Get("EXECUTOR_FRONTEND_PASSWORD", "", "The password supplied to the frontend.")
	c.QueueName = c.Get("EXECUTOR_QUEUE_NAME", "", "The name of the queue to listen to.")
	c.QueuePollInterval = c.GetInterval("EXECUTOR_QUEUE_POLL_INTERVAL", "1s", "Interval between dequeue requests.")
	c.HeartbeatInterval = c.GetInterval("EXECUTOR_HEARTBEAT_INTERVAL", "1s", "Interval between heartbeat requests.")
	c.MaximumNumJobs = c.GetInt("EXECUTOR_MAXIMUM_NUM_JOBS", "1", "Number of virtual machines or containers that can be running at once.")
	c.UseFirecracker = c.GetBool("EXECUTOR_USE_FIRECRACKER", "true", "Whether to isolate commands in virtual machines.")
	c.FirecrackerImage = c.Get("EXECUTOR_FIRECRACKER_IMAGE", "sourcegraph/ignite-ubuntu:insiders", "The base image to use for virtual machines.")
	c.FirecrackerNumCPUs = c.GetInt("EXECUTOR_FIRECRACKER_NUM_CPUS", "4", "How many CPUs to allocate to each virtual machine or container.")
	c.FirecrackerMemory = c.Get("EXECUTOR_FIRECRACKER_MEMORY", "12G", "How much memory to allocate to each virtual machine or container.")
	c.FirecrackerDiskSpace = c.Get("EXECUTOR_FIRECRACKER_DISK_SPACE", "20G", "How much disk space to allocate to each virtual machine or container.")
	c.ImageArchivesPath = c.Get("EXECUTOR_IMAGE_ARCHIVE_PATH", "", "Where to store tar archives of docker images shared by virtual machines.")
	c.DisableHealthServer = c.GetBool("EXECUTOR_DISABLE_HEALTHSERVER", "false", "Whether or not to disable the health server.")
}

func (c *Config) APIWorkerOptions(transport http.RoundTripper) apiworker.Options {
	return apiworker.Options{
		QueueName:          c.QueueName,
		HeartbeatInterval:  c.HeartbeatInterval,
		WorkerOptions:      c.WorkerOptions(),
		FirecrackerOptions: c.FirecrackerOptions(),
		ResourceOptions:    c.ResourceOptions(),
		GitServicePath:     "/.executors/git",
		ClientOptions:      c.ClientOptions(transport),
	}
}

func (c *Config) WorkerOptions() workerutil.WorkerOptions {
	return workerutil.WorkerOptions{
		NumHandlers: c.MaximumNumJobs,
		Interval:    c.QueuePollInterval,
		Metrics:     makeWorkerMetrics(),
	}
}

func (c *Config) FirecrackerOptions() command.FirecrackerOptions {
	return command.FirecrackerOptions{
		Enabled:           c.UseFirecracker,
		Image:             c.FirecrackerImage,
		ImageArchivesPath: c.ImageArchivesPath,
	}
}

func (c *Config) ResourceOptions() command.ResourceOptions {
	return command.ResourceOptions{
		NumCPUs:   c.FirecrackerNumCPUs,
		Memory:    c.FirecrackerMemory,
		DiskSpace: c.FirecrackerDiskSpace,
	}
}

func (c *Config) ClientOptions(transport http.RoundTripper) apiclient.Options {
	return apiclient.Options{
		ExecutorName:      uuid.New().String(),
		PathPrefix:        "/.executors/queue",
		EndpointOptions:   c.EndpointOptions(),
		BaseClientOptions: c.BaseClientOptions(transport),
	}
}

func (c *Config) BaseClientOptions(transport http.RoundTripper) apiclient.BaseClientOptions {
	return apiclient.BaseClientOptions{
		TraceOperationName: "Executor Queue Client",
		Transport:          transport,
	}
}

func (c *Config) EndpointOptions() apiclient.EndpointOptions {
	return apiclient.EndpointOptions{
		URL:      c.FrontendURL,
		Username: c.FrontendUsername,
		Password: c.FrontendPassword,
	}
}
