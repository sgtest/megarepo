package build

import (
	"fmt"
	"sync"

	"github.com/buildkite/go-buildkite/v3/buildkite"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/dev/build-tracker/notify"
	"github.com/sourcegraph/sourcegraph/dev/build-tracker/util"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Build keeps track of a buildkite.Build and it's associated jobs and pipeline.
// See BuildStore for where jobs are added to the build.
type Build struct {
	// Build is the buildkite.Build currently being executed by buildkite on a particular Pipeline
	buildkite.Build `json:"build"`

	// Pipeline is a wrapped buildkite.Pipeline that is running this build.
	Pipeline *Pipeline `json:"pipeline"`

	// steps is a map that keeps track of all the buildkite.Jobs associated with this build.
	// Each step keeps track of jobs associated with that step. Every job is wrapped to allow
	// for safer access to fields of the buildkite.Jobs. The name of the job is used as the key
	Steps map[string]*Step `json:"steps"`

	// ConsecutiveFailure indicates whether this build is the nth consecutive failure.
	ConsecutiveFailure int `json:"consecutiveFailures"`

	// Mutex is used to to control and stop other changes being made to the build.
	sync.Mutex
}

type Step struct {
	Name string `json:"steps"`
	Jobs []*Job `json:"jobs"`
}

// Implement the notify.JobLine interface
var _ notify.JobLine = &Step{}

func (s *Step) Title() string {
	return s.Name
}

func (s *Step) LogURL() string {
	return s.LastJob().WebURL
}

// BuildStatus is the status of the build. The status is determined by the final status of contained Jobs of the build
type BuildStatus string

const (
	BuildStatusUnknown BuildStatus = ""
	BuildPassed        BuildStatus = "Passed"
	BuildFailed        BuildStatus = "Failed"
	BuildFixed         BuildStatus = "Fixed"
)

func (b *Build) AddJob(j *Job) error {
	stepName := j.GetName()
	if stepName == "" {
		return errors.Newf("job %q name is empty", j.GetID())
	}
	step, ok := b.Steps[stepName]
	// We don't know about this step, so it must be a new one
	if !ok {
		step = NewStep(stepName)
		b.Steps[step.Name] = step
	}
	step.Jobs = append(step.Jobs, j)
	return nil
}

// updateFromEvent updates the current build with the build and pipeline from the event.
func (b *Build) updateFromEvent(e *Event) {
	b.Build = e.Build
	b.Pipeline = e.WrappedPipeline()
}

func (b *Build) IsFailed() bool {
	return b.GetState() == "failed"
}

func (b *Build) IsFinished() bool {
	switch b.GetState() {
	case "passed", "failed", "blocked", "canceled":
		return true
	default:
		return false
	}
}

func (b *Build) GetAuthorName() string {
	if b.Author == nil {
		return ""
	}

	return b.Author.Name
}

func (b *Build) GetAuthorEmail() string {
	if b.Author == nil {
		return ""
	}

	return b.Author.Email
}

func (b *Build) GetState() string {
	return util.Strp(b.State)
}

func (b *Build) GetCommit() string {
	return util.Strp(b.Commit)
}

func (b *Build) GetNumber() int {
	return util.Intp(b.Number)
}

func (b *Build) GetBranch() string {
	return util.Strp(b.Branch)
}

func (b *Build) GetMessage() string {
	return util.Strp(b.Message)
}

// Pipeline wraps a buildkite.Pipeline and provides convenience functions to access values of the wrapped pipeline in a safe maner
type Pipeline struct {
	buildkite.Pipeline `json:"pipeline"`
}

func (p *Pipeline) GetName() string {
	if p == nil {
		return ""
	}
	return util.Strp(p.Name)
}

// Event contains information about a buildkite event. Each event contains the build, pipeline, and job. Note that when the event
// is `build.*` then Job will be empty.
type Event struct {
	// Name is the name of the buildkite event that got triggered
	Name string `json:"event"`
	// Build is the buildkite.Build that triggered this event
	Build buildkite.Build `json:"build,omitempty"`
	// Pipeline is the buildkite.Pipeline that is running the build that triggered this event
	Pipeline buildkite.Pipeline `json:"pipeline,omitempty"`
	// Job is the current job being executed by the Build. When the event is not a job event variant, then this job will be empty
	Job buildkite.Job `json:"job,omitempty"`
}

func (b *Event) WrappedBuild() *Build {
	build := &Build{
		Build:    b.Build,
		Pipeline: b.WrappedPipeline(),
		Steps:    make(map[string]*Step),
	}

	return build
}

func (b *Event) WrappedJob() *Job {
	return &Job{Job: b.Job}
}

func (b *Event) WrappedPipeline() *Pipeline {
	return &Pipeline{Pipeline: b.Pipeline}
}

func (b *Event) IsBuildFinished() bool {
	return b.Name == "build.finished"
}

func (b *Event) IsJobFinished() bool {
	return b.Name == "job.finished"
}

func (b *Event) GetJobName() string {
	return util.Strp(b.Job.Name)
}

func (b *Event) GetBuildNumber() int {
	return util.Intp(b.Build.Number)
}

// Store is a thread safe store which keeps track of Builds described by buildkite build events.
//
// The store is backed by a map and the build number is used as the key.
// When a build event is added the Buildkite Build, Pipeline and Job is extracted, if available. If the Build does not exist, Buildkite is wrapped
// in a Build and added to the map. When the event contains a Job the corresponding job is retrieved from the map and added to the Job it is for.
type Store struct {
	logger log.Logger

	builds map[int]*Build
	// consecutiveFailures tracks how many consecutive build failed events has been
	// received by pipeline and branch
	consecutiveFailures map[string]int

	// m locks all writes to BuildStore properties.
	m sync.RWMutex
}

func NewBuildStore(logger log.Logger) *Store {
	return &Store{
		logger: logger.Scoped("store", "stores all the buildkite builds"),

		builds:              make(map[int]*Build),
		consecutiveFailures: make(map[string]int),

		m: sync.RWMutex{},
	}
}

func (s *Store) Add(event *Event) {
	s.m.Lock()
	defer s.m.Unlock()

	build, ok := s.builds[event.GetBuildNumber()]
	// if we don't know about this build, convert it and add it to the store
	if !ok {
		build = event.WrappedBuild()
		s.builds[event.GetBuildNumber()] = build
	}

	// Now that we have a build, lets make sure it isn't modified while we look and possibly update it
	build.Lock()
	defer build.Unlock()

	// if the build is finished replace the original build with the replaced one since it
	// will be more up to date, and tack on some finalized data
	if event.IsBuildFinished() {
		build.updateFromEvent(event)

		// Track consecutive failures by pipeline + branch
		// We update the global count of consecutiveFailures then we set the count on the individual build
		// if we get a pass, we reset the global count of consecutiveFailures
		failuresKey := fmt.Sprintf("%s/%s", build.Pipeline.GetName(), build.GetBranch())
		if build.IsFailed() {
			s.consecutiveFailures[failuresKey] += 1
			build.ConsecutiveFailure = s.consecutiveFailures[failuresKey]
		} else {
			// We got a pass, reset the global count
			s.consecutiveFailures[failuresKey] = 0
		}
	}

	// Keep track of the job, if there is one
	newJob := event.WrappedJob()
	err := build.AddJob(newJob)
	if err != nil {
		s.logger.Warn("job for step has no name - not added",
			log.Int("buildNumber", event.GetBuildNumber()),
			log.Object("job", log.String("name", newJob.GetName()), log.String("id", newJob.GetID())),
			log.Int("totalSteps", len(build.Steps)),
		)
	} else {
		s.logger.Debug("job added to step",
			log.Int("buildNumber", event.GetBuildNumber()),
			log.Object("step", log.String("name", newJob.GetName()),
				log.Object("job", log.String("state", newJob.state()), log.String("id", newJob.GetID())),
			),
			log.Int("totalSteps", len(build.Steps)),
		)

	}

}

func (s *Store) Set(build *Build) {
	s.m.RLock()
	defer s.m.RUnlock()
	s.builds[build.GetNumber()] = build
}

func (s *Store) GetByBuildNumber(num int) *Build {
	s.m.RLock()
	defer s.m.RUnlock()

	return s.builds[num]
}

func (s *Store) DelByBuildNumber(buildNumbers ...int) {
	s.m.Lock()
	defer s.m.Unlock()

	for _, num := range buildNumbers {
		delete(s.builds, num)
	}
	s.logger.Info("deleted builds", log.Int("totalBuilds", len(buildNumbers)))
}

func (s *Store) FinishedBuilds() []*Build {
	s.m.RLock()
	defer s.m.RUnlock()

	finished := make([]*Build, 0)
	for _, b := range s.builds {
		if b.IsFinished() {
			s.logger.Debug("build is finished", log.Int("buildNumber", b.GetNumber()), log.String("state", b.GetState()))
			finished = append(finished, b)
		}
	}

	return finished
}
