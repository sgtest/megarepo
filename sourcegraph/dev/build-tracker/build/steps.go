package build

import (
	"strings"

	"github.com/buildkite/go-buildkite/v3/buildkite"

	"github.com/sourcegraph/sourcegraph/dev/build-tracker/util"
)

type JobStatus string

const (
	JobFixed  JobStatus = JobStatus(BuildFixed)
	JobFailed JobStatus = JobStatus(BuildFailed)
	JobPassed JobStatus = JobStatus(BuildPassed)
)

func (js JobStatus) ToBuildStatus() BuildStatus {
	return BuildStatus(js)
}

type Job struct {
	buildkite.Job
}

func (j *Job) GetID() string {
	return util.Strp(j.ID)
}

func (j *Job) GetName() string {
	return util.Strp(j.Name)
}

func (j *Job) exitStatus() int {
	return util.Intp(j.ExitStatus)
}

func (j *Job) failed() bool {
	return !j.SoftFailed && j.exitStatus() > 0
}

func (j *Job) state() string {
	return strings.ToTitle(util.Strp(j.State))
}

func (j *Job) status() JobStatus {
	if j.failed() {
		return JobFailed
	}
	return JobPassed
}

func (j *Job) hasTimedOut() bool {
	return j.state() == "timed_out"
}

func NewStep(name string) *Step {
	return &Step{
		Name: name,
		Jobs: make([]*Job, 0),
	}
}

func NewStepFromJob(j *Job) *Step {
	s := NewStep(j.GetName())
	s.Add(j)
	return s
}

func (s *Step) Add(j *Job) {
	s.Jobs = append(s.Jobs, j)
}

func (s *Step) FinalStatus() JobStatus {
	// If we have no jobs for some reason, then we regard it as the StepState as Passed ... cannot have a Failed StepState
	// if we have no jobs!
	if len(s.Jobs) == 0 {
		return JobPassed
	}
	if len(s.Jobs) == 1 {
		return s.LastJob().status()
	}
	// we only care about the last two states of because that determines the final state
	// n - 1  |   n    | Final
	// Passed | Passed | Passed
	// Passed | Failed | Failed
	// Failed | Failed | Failed
	// Failed | Passed | Fixed
	secondLastStatus := s.Jobs[len(s.Jobs)-2].status()
	lastStatus := s.Jobs[len(s.Jobs)-1].status()

	// Note that for all cases except the last case, the final state is whatever the last job state is.
	// The final state only differs when the before state is Failed and the last State is Passed, so
	finalState := lastStatus
	if secondLastStatus == JobFailed && lastStatus == JobPassed {
		finalState = JobFixed
	}

	return finalState
}

func (s *Step) LastJob() *Job {
	return s.Jobs[len(s.Jobs)-1]
}

func FindFailedSteps(steps map[string]*Step) []*Step {
	results := []*Step{}

	for _, step := range steps {
		if state := step.FinalStatus(); state == JobFailed {
			results = append(results, step)
		}
	}
	return results
}

func GroupByStatus(steps map[string]*Step) map[JobStatus][]*Step {
	groups := make(map[JobStatus][]*Step)

	for _, step := range steps {
		state := step.FinalStatus()

		items, ok := groups[state]
		if !ok {
			items = make([]*Step, 0)
		}
		groups[state] = append(items, step)
	}

	return groups
}
