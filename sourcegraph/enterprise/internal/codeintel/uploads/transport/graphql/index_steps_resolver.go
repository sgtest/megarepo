package graphql

import (
	"context"
	"fmt"

	"github.com/grafana/regexp"

	sharedresolvers "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/resolvers"
	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	resolverstubs "github.com/sourcegraph/sourcegraph/internal/codeintel/resolvers"
	"github.com/sourcegraph/sourcegraph/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
)

// indexStepsResolver resolves the steps of an index record.
//
// Index jobs are broken into three parts:
//   - pre-index steps; all but the last docker step
//   - index step; the last docker step
//   - upload step; the only src-cli step
//
// The setup and teardown steps match the executor setup and teardown.
type indexStepsResolver struct {
	siteAdminChecker sharedresolvers.SiteAdminChecker
	index            uploadsshared.Index
}

func NewIndexStepsResolver(siteAdminChecker sharedresolvers.SiteAdminChecker, index uploadsshared.Index) resolverstubs.IndexStepsResolver {
	return &indexStepsResolver{siteAdminChecker: siteAdminChecker, index: index}
}

func (r *indexStepsResolver) Setup() []resolverstubs.ExecutionLogEntryResolver {
	return r.executionLogEntryResolversWithPrefix(logKeyPrefixSetup)
}

var logKeyPrefixSetup = regexp.MustCompile("^setup\\.")

func (r *indexStepsResolver) PreIndex() []resolverstubs.PreIndexStepResolver {
	var resolvers []resolverstubs.PreIndexStepResolver
	for i, step := range r.index.DockerSteps {
		logKeyPreIndex := regexp.MustCompile(fmt.Sprintf("step\\.(docker|kubernetes)\\.pre-index\\.%d", i))
		if entry, ok := r.findExecutionLogEntry(logKeyPreIndex); ok {
			resolvers = append(resolvers, newPreIndexStepResolver(r.siteAdminChecker, step, &entry))
			// This is here for backwards compatibility for records that were created before
			// named keys for steps existed.
		} else if entry, ok := r.findExecutionLogEntry(regexp.MustCompile(fmt.Sprintf("step\\.(docker|kubernetes)\\.%d", i))); ok {
			resolvers = append(resolvers, newPreIndexStepResolver(r.siteAdminChecker, step, &entry))
		} else {
			resolvers = append(resolvers, newPreIndexStepResolver(r.siteAdminChecker, step, nil))
		}
	}

	return resolvers
}

func (r *indexStepsResolver) Index() resolverstubs.IndexStepResolver {
	if entry, ok := r.findExecutionLogEntry(logKeyPrefixIndexer); ok {
		return newIndexStepResolver(r.siteAdminChecker, r.index, &entry)
	}

	// This is here for backwards compatibility for records that were created before
	// named keys for steps existed.
	logKeyRegex := regexp.MustCompile(fmt.Sprintf("^step\\.(docker|kubernetes)\\.%d", len(r.index.DockerSteps)))
	if entry, ok := r.findExecutionLogEntry(logKeyRegex); ok {
		return newIndexStepResolver(r.siteAdminChecker, r.index, &entry)
	}

	return newIndexStepResolver(r.siteAdminChecker, r.index, nil)
}

var logKeyPrefixIndexer = regexp.MustCompile("^step\\.(docker|kubernetes)\\.indexer")

func (r *indexStepsResolver) Upload() resolverstubs.ExecutionLogEntryResolver {
	if entry, ok := r.findExecutionLogEntry(logKeyPrefixUpload); ok {
		return newExecutionLogEntryResolver(r.siteAdminChecker, entry)
	}

	// This is here for backwards compatibility for records that were created before
	// named keys for steps existed.
	if entry, ok := r.findExecutionLogEntry(logKeyPrefixSrcFirstStep); ok {
		return newExecutionLogEntryResolver(r.siteAdminChecker, entry)
	}

	return nil
}

var (
	logKeyPrefixUpload       = regexp.MustCompile("^step\\.(docker|kubernetes|src)\\.upload")
	logKeyPrefixSrcFirstStep = regexp.MustCompile("^step\\.src\\.0")
)

func (r *indexStepsResolver) Teardown() []resolverstubs.ExecutionLogEntryResolver {
	return r.executionLogEntryResolversWithPrefix(logKeyPrefixTeardown)
}

var logKeyPrefixTeardown = regexp.MustCompile("^teardown\\.")

func (r *indexStepsResolver) findExecutionLogEntry(key *regexp.Regexp) (executor.ExecutionLogEntry, bool) {
	for _, entry := range r.index.ExecutionLogs {
		if key.MatchString(entry.Key) {
			return entry, true
		}
	}

	return executor.ExecutionLogEntry{}, false
}

func (r *indexStepsResolver) executionLogEntryResolversWithPrefix(prefix *regexp.Regexp) []resolverstubs.ExecutionLogEntryResolver {
	var resolvers []resolverstubs.ExecutionLogEntryResolver
	for _, entry := range r.index.ExecutionLogs {
		if prefix.MatchString(entry.Key) {
			res := newExecutionLogEntryResolver(r.siteAdminChecker, entry)
			resolvers = append(resolvers, res)
		}
	}

	return resolvers
}

//
//

type preIndexStepResolver struct {
	siteAdminChecker sharedresolvers.SiteAdminChecker
	step             uploadsshared.DockerStep
	entry            *executor.ExecutionLogEntry
}

func newPreIndexStepResolver(siteAdminChecker sharedresolvers.SiteAdminChecker, step uploadsshared.DockerStep, entry *executor.ExecutionLogEntry) resolverstubs.PreIndexStepResolver {
	return &preIndexStepResolver{
		siteAdminChecker: siteAdminChecker,
		step:             step,
		entry:            entry,
	}
}

func (r *preIndexStepResolver) Root() string       { return r.step.Root }
func (r *preIndexStepResolver) Image() string      { return r.step.Image }
func (r *preIndexStepResolver) Commands() []string { return r.step.Commands }

func (r *preIndexStepResolver) LogEntry() resolverstubs.ExecutionLogEntryResolver {
	if r.entry != nil {
		return newExecutionLogEntryResolver(r.siteAdminChecker, *r.entry)
	}

	return nil
}

//
//

type indexStepResolver struct {
	siteAdminChecker sharedresolvers.SiteAdminChecker
	index            uploadsshared.Index
	entry            *executor.ExecutionLogEntry
}

func newIndexStepResolver(siteAdminChecker sharedresolvers.SiteAdminChecker, index uploadsshared.Index, entry *executor.ExecutionLogEntry) resolverstubs.IndexStepResolver {
	return &indexStepResolver{
		siteAdminChecker: siteAdminChecker,
		index:            index,
		entry:            entry,
	}
}

func (r *indexStepResolver) Commands() []string    { return r.index.LocalSteps }
func (r *indexStepResolver) IndexerArgs() []string { return r.index.IndexerArgs }
func (r *indexStepResolver) Outfile() *string      { return resolverstubs.NonZeroPtr(r.index.Outfile) }

func (r *indexStepResolver) RequestedEnvVars() *[]string {
	if len(r.index.RequestedEnvVars) == 0 {
		return nil
	}
	return &r.index.RequestedEnvVars
}

func (r *indexStepResolver) LogEntry() resolverstubs.ExecutionLogEntryResolver {
	if r.entry != nil {
		return newExecutionLogEntryResolver(r.siteAdminChecker, *r.entry)
	}

	return nil
}

//
//

type executionLogEntryResolver struct {
	entry            executor.ExecutionLogEntry
	siteAdminChecker sharedresolvers.SiteAdminChecker
}

func newExecutionLogEntryResolver(siteAdminChecker sharedresolvers.SiteAdminChecker, entry executor.ExecutionLogEntry) resolverstubs.ExecutionLogEntryResolver {
	return &executionLogEntryResolver{
		entry:            entry,
		siteAdminChecker: siteAdminChecker,
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

func (r *executionLogEntryResolver) StartTime() gqlutil.DateTime {
	return gqlutil.DateTime{Time: r.entry.StartTime}
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
	if err := r.siteAdminChecker.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		if err != auth.ErrMustBeSiteAdmin {
			return "", err
		}

		return "", nil
	}

	return r.entry.Out, nil
}
