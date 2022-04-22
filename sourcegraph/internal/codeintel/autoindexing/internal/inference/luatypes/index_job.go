package luatypes

import (
	lua "github.com/yuin/gopher-lua"

	"github.com/sourcegraph/sourcegraph/internal/luasandbox/util"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/autoindex/config"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// IndexJobsFromTable decodes a single index job or slice of index jobs from the given Lua
// value.
func IndexJobsFromTable(value lua.LValue) (patterns []config.IndexJob, err error) {
	err = util.UnwrapSliceOrSingleton(value, func(value lua.LValue) error {
		job, err := indexJobFromTable(value)
		if err != nil {
			return err
		}

		patterns = append(patterns, job)
		return nil
	})

	return
}

// indexJobFromTable decodes a single Lua table value into an index job instance.
func indexJobFromTable(value lua.LValue) (config.IndexJob, error) {
	table, ok := value.(*lua.LTable)
	if !ok {
		return config.IndexJob{}, util.NewTypeError("table", value)
	}

	job := config.IndexJob{}
	if err := util.DecodeTable(table, map[string]func(lua.LValue) error{
		"steps":        setDockerSteps(&job.Steps),
		"local_steps":  util.SetStrings(&job.LocalSteps),
		"root":         util.SetString(&job.Root),
		"indexer":      util.SetString(&job.Indexer),
		"indexer_args": util.SetStrings(&job.IndexerArgs),
		"outfile":      util.SetString(&job.Outfile),
	}); err != nil {
		return config.IndexJob{}, err
	}

	if job.Indexer == "" {
		return config.IndexJob{}, errors.Newf("no indexer supplied")
	}

	return job, nil
}

// dockerStepFromTable decodes a single Lua table value into a docker steps instance.
func dockerStepFromTable(value lua.LValue) (step config.DockerStep, _ error) {
	table, ok := value.(*lua.LTable)
	if !ok {
		return config.DockerStep{}, util.NewTypeError("table", value)
	}

	if err := util.DecodeTable(table, map[string]func(lua.LValue) error{
		"root":     util.SetString(&step.Root),
		"image":    util.SetString(&step.Image),
		"commands": util.SetStrings(&step.Commands),
	}); err != nil {
		return config.DockerStep{}, err
	}

	if step.Image == "" {
		return config.DockerStep{}, errors.Newf("no image supplied")
	}

	return step, nil
}

// setDockerSteps returns a decoder function that updates the given docker step
// slice value on invocation. For use in luasandbox.DecodeTable.
func setDockerSteps(ptr *[]config.DockerStep) func(lua.LValue) error {
	return func(value lua.LValue) (err error) {
		values, err := util.DecodeSlice(value)
		if err != nil {
			return err
		}

		for _, v := range values {
			step, err := dockerStepFromTable(v)
			if err != nil {
				return err
			}
			*ptr = append(*ptr, step)
		}

		return nil
	}
}
