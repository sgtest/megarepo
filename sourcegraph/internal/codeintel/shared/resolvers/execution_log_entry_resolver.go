package sharedresolvers

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

type ExecutionLogEntryResolver interface {
	Key() string
	Command() []string
	StartTime() DateTime
	ExitCode() *int32
	Out(ctx context.Context) (string, error)
	DurationMilliseconds() *int32
}

type executionLogEntryResolver struct {
	svc   AutoIndexingService
	entry workerutil.ExecutionLogEntry
}

func NewExecutionLogEntryResolver(svc AutoIndexingService, entry workerutil.ExecutionLogEntry) ExecutionLogEntryResolver {
	return &executionLogEntryResolver{
		svc:   svc,
		entry: entry,
	}
}

func (r *executionLogEntryResolver) Key() string       { return r.entry.Key }
func (r *executionLogEntryResolver) Command() []string { return r.entry.Command }

func (r *executionLogEntryResolver) ExitCode() *int32 {
	if r.entry.ExitCode == nil {
		return nil
	}
	val := int32(*r.entry.ExitCode)
	return &val
}

func (r *executionLogEntryResolver) StartTime() DateTime {
	return DateTime{Time: r.entry.StartTime}
}

func (r *executionLogEntryResolver) DurationMilliseconds() *int32 {
	if r.entry.DurationMs == nil {
		return nil
	}
	val := int32(*r.entry.DurationMs)
	return &val
}

func (r *executionLogEntryResolver) Out(ctx context.Context) (string, error) {
	// 🚨 SECURITY: Only site admins can view executor log contents.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.svc.GetUnsafeDB()); err != nil {
		if err != auth.ErrMustBeSiteAdmin {
			return "", err
		}

		return "", nil
	}

	return r.entry.Out, nil
}
