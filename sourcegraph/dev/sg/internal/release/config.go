package release

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"strings"

	"github.com/sourcegraph/run"
	"gopkg.in/yaml.v3"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

// TODO sg release scaffold ...
// TODO add PR body
type ReleaseManifest struct {
	// Meta defines information about the product being released, so we can
	// track who is in charge of releasing this, what kind of artifacts is it producting,
	// etc ...
	Meta struct {
		ProductName string   `yaml:"productName"`
		Owners      []string `yaml:"owners"`
		Repository  string   `yaml:"repository"`
		Artifacts   []string `yaml:"artifacts"`
		README      string   `yaml:"README"`
	} `yaml:"meta"`
	// Requirements is a list of commands that must exit without errors for the manifest to be
	// considered to be valid. Alternatively, instead of defining Cmd, Env can be set to
	// ensure an environment variable is defnined.
	Requirements []requirement `yaml:"requirements,omitempty"`
	// Inputs defines a list of k=v strings, defining the required inputs for that release manifest.
	// Typically, this is either empty or server=vX.Y.Z to build a release that uses that particular
	// server version.
	Inputs []input `yaml:"inputs"`
	// Internal defines the steps to create an internal release.
	Internal struct {
		// Create defines the steps to create the release build. This is where we define changes that need
		// to be applied on the code for the release to exist. Typically, that means updating images,
		// fetching new container tags, etc ...
		Create struct {
			Steps struct {
				Patch []cmdManifest `yaml:"patch"`
				Minor []cmdManifest `yaml:"minor"`
				Major []cmdManifest `yaml:"major"`
			} `yaml:"steps"`
		} `yaml:"create"`
		// Finalize defines the steps to execute once the internal release build and test phases have been successfully completed.
		// Typically, this is where one would define commands to open a PR on a documentation repo to take note of this
		// new release.
		Finalize struct {
			Steps []cmdManifest `yaml:"steps"`
		} `yaml:"finalize"`
	} `yaml:"internal"`
	// Test defines the steps to test the release build. These are not meant to be "normal tests", but instead
	// extended testing to ensure the release is correct. These tests are to be executed both during the
	// create and promote-to-public phase.
	Test struct {
		Steps []cmdManifest `yaml:"steps"`
	} `yaml:"test"`
	// PromoteToPublic defines steps to execute when promoting the release to a public one. Typically that's where
	// one would move release artifacts from a private place to one that is publicly accessible.
	PromoteToPublic struct {
		Create struct {
			Steps []cmdManifest `yaml:"steps"`
		} `yaml:"create"`
		Finalize struct {
			Steps []cmdManifest `yaml:"steps"`
		} `yaml:"finalize"`
	} `yaml:"promoteToPublic"`
}

type requirement struct {
	Name            string `yaml:"name"`
	Cmd             string `yaml:"cmd"`
	Env             string `yaml:"env"`
	FixInstructions string `yaml:"fixInstructions"`
	// Only allows to check a requirement just for a specific stage.
	Only []string `yaml:"only,omitempty"`
}

const (
	stageInternalCreate   = "internal.create"
	stageInternalFinalize = "internal.finalize"
	stageTest             = "test"
	stagePromoteCreate    = "promoteToPublic.create"
	stagePromoteFinalize  = "promoteToPublic.finalize"
)

func validateRequirementOnly(only []string) error {
	if len(only) == 0 {
		return nil
	}
	for _, str := range only {
		switch str {
		case stageInternalCreate:
			continue
		case stageInternalFinalize:
			continue
		case stageTest:
			continue
		case stagePromoteCreate:
			continue
		case stagePromoteFinalize:
			continue
		default:
			return errors.Newf("invalid only value: %q", str)
		}
	}
	return nil
}

type cmdManifest struct {
	Name string `yaml:"name"`
	Cmd  string `yaml:"cmd"`
}

type input struct {
	ReleaseID string `yaml:"releaseId"`
}

type releaseRunner struct {
	vars          map[string]string
	inputs        map[string]string
	m             *ReleaseManifest
	version       string
	pretend       bool
	typ           string
	isDevelopment bool
}

// releaseConfig is a serializable structure holding the configuration
// for the release tooling, that can be passed around easily.
type releaseConfig struct {
	Version string `json:"version"`
	Inputs  string `json:"inputs"`
	Type    string `json:"type"`
}

func parseReleaseConfig(configRaw string) (*releaseConfig, error) {
	rc := releaseConfig{}
	if err := json.Unmarshal([]byte(configRaw), &rc); err != nil {
		return nil, err
	}
	return &rc, nil
}

func NewReleaseRunner(ctx context.Context, workdir string, version string, inputsArg string, typ string, gitBranch string, pretend, isDevelopment bool) (*releaseRunner, error) {
	announce2("setup", "Finding release manifest in %q", workdir)

	inputs, err := parseInputs(inputsArg)
	if err != nil {
		return nil, err
	}

	config := releaseConfig{
		Version: version,
		Inputs:  inputsArg,
		Type:    typ,
	}

	configBytes, err := json.Marshal(config)
	if err != nil {
		return nil, err
	}

	if gitBranch == "" {
		cmd := run.Cmd(ctx, "git rev-parse --abbrev-ref HEAD")
		cmd.Dir(workdir)
		out, err := cmd.Run().String()
		if err != nil {
			return nil, err
		}
		gitBranch = out
		sayWarn("setup", "No explicit branch name was provided, assuming current branch is the target: %s", gitBranch)
	}

	vars := map[string]string{
		"version":        version,
		"tag":            strings.TrimPrefix(version, "v"),
		"config":         string(configBytes),
		"git.branch":     gitBranch,
		"is_development": strconv.FormatBool(isDevelopment),
	}
	for k, v := range inputs {
		// TODO sanitize input format
		vars[fmt.Sprintf("inputs.%s.version", k)] = v
		vars[fmt.Sprintf("inputs.%s.tag", k)] = strings.TrimPrefix(v, "v")
	}

	if err := os.Chdir(workdir); err != nil {
		return nil, err
	}

	f, err := os.Open("release.yaml")
	if err != nil {
		say("setup", "failed to find release manifest")
		return nil, err
	}
	defer f.Close()

	var m ReleaseManifest
	dec := yaml.NewDecoder(f)
	if err := dec.Decode(&m); err != nil {
		say("setup", "failed to decode manifest")
		return nil, err
	}
	saySuccess("setup", "Found manifest for %q (%s)", m.Meta.ProductName, m.Meta.Repository)

	say("meta", "Owners: %s", strings.Join(m.Meta.Owners, ", "))
	say("meta", "Repository: %s", m.Meta.Repository)

	for _, in := range m.Inputs {
		var found bool
		for k := range inputs {
			if k == in.ReleaseID {
				found = true
			}
		}
		if !found {
			sayFail("inputs", "Couldn't find input %q, required by manifest, but --inputs=%s=... flag is missing", in.ReleaseID, in.ReleaseID)
			return nil, errors.New("missing input")
		}
	}

	announce2("vars", "Variables")
	for k, v := range vars {
		say("vars", "%s=%q", k, v)
	}

	r := &releaseRunner{
		version:       version,
		pretend:       pretend,
		inputs:        inputs,
		typ:           typ,
		m:             &m,
		vars:          vars,
		isDevelopment: isDevelopment,
	}

	return r, nil
}

func shouldSkipReqCheck(req requirement, stage string) bool {
	if len(req.Only) == 0 {
		return false
	}
	for _, o := range req.Only {
		if o == stage {
			return false
		}
	}
	return true
}

func (r *releaseRunner) checkRequirements(ctx context.Context, stage string) error {
	announce2("reqs", "Checking requirements...")

	if len(r.m.Requirements) == 0 {
		saySuccess("reqs", "Requirement checks skipped, no requirements defined.")
	}

	var failed bool
	for _, req := range r.m.Requirements {
		if shouldSkipReqCheck(req, stage) {
			saySuccess("reqs", "🔕 %s (excluded for %s)", req.Name, stage)
			continue
		}

		if req.Env != "" && req.Cmd != "" {
			return errors.Newf("requirement %q can't have both env and cmd defined", req.Name)
		}
		if req.Env != "" {
			if _, ok := os.LookupEnv(req.Env); !ok {
				failed = true
				sayFail("reqs", "❌ %s, $%s is not defined.", req.Name, req.Env)
				continue
			}
			saySuccess("reqs", "✅ %s", req.Name)
			continue
		}

		lines, err := run.Cmd(ctx, req.Cmd).Run().Lines()
		if err != nil {
			failed = true
			sayFail("reqs", "❌ %s", req.Name)
			sayFail("reqs", "  Error: %s", err.Error())
			for _, line := range lines {
				sayFail("reqs", "  "+line)
			}
		} else {
			saySuccess("reqs", "✅ %s", req.Name)
		}
	}
	if failed {
		announce2("reqs", "Requirement checks failed, aborting.")
		return errors.New("failed requirements")
	}
	return nil
}

func (r *releaseRunner) CreateRelease(ctx context.Context) error {
	if err := r.checkRequirements(ctx, stageInternalCreate); err != nil {
		return err
	}

	var steps []cmdManifest
	switch r.typ {
	case "patch":
		steps = r.m.Internal.Create.Steps.Patch
	case "minor":
		steps = r.m.Internal.Create.Steps.Minor
	case "major":
		steps = r.m.Internal.Create.Steps.Major
	}

	// We don't want to accidentally think the release creation worked if there are no steps defined.
	if len(steps) == 0 {
		sayFail("create", "No steps defined for %s release", r.typ)
		return errors.Newf("no steps defined for %s release", r.typ)
	}

	announce2("create", "Will create a %s release %q", r.typ, r.version)
	return r.runSteps(ctx, steps)
}

func (r *releaseRunner) InternalFinalize(ctx context.Context) error {
	if err := r.checkRequirements(ctx, stageInternalFinalize); err != nil {
		return err
	}

	if len(r.m.Internal.Finalize.Steps) == 0 {
		announce2("finalize", "Skipping internal release finalization, none defined")
		return nil
	}
	announce2("finalize", "Running finalize steps for %s", r.version)
	return r.runSteps(ctx, r.m.Internal.Finalize.Steps)
}

func (r *releaseRunner) Test(ctx context.Context) error {
	if err := r.checkRequirements(ctx, stageTest); err != nil {
		return err
	}

	if len(r.m.Test.Steps) == 0 {
		announce2("test", "Skipping release tests, none defined")
		return nil
	}
	announce2("test", "Running testing steps for %s", r.version)
	return r.runSteps(ctx, r.m.Test.Steps)
}

func (r *releaseRunner) Promote(ctx context.Context) error {
	if r.isDevelopment {
		return errors.New("cannot promote a development release")
	}

	if err := r.checkRequirements(ctx, stagePromoteCreate); err != nil {
		return err
	}
	announce2("promote", "Will promote %q to a public release", r.version)
	return r.runSteps(ctx, r.m.PromoteToPublic.Create.Steps)
}

func (r *releaseRunner) PromoteFinalize(ctx context.Context) error {
	if r.isDevelopment {
		return errors.New("cannot promote a development release")
	}

	if err := r.checkRequirements(ctx, stagePromoteFinalize); err != nil {
		return err
	}

	if len(r.m.PromoteToPublic.Finalize.Steps) == 0 {
		announce2("finalize", "Skipping public release finalization, none defined")
		return nil
	}
	announce2("finalize", "Running promote finalize steps for %s", r.version)
	return r.runSteps(ctx, r.m.PromoteToPublic.Finalize.Steps)
}

func (r *releaseRunner) runSteps(ctx context.Context, steps []cmdManifest) error {
	for _, step := range steps {
		cmd := interpolate(step.Cmd, r.vars)
		if r.pretend {
			announce2("step", "Pretending to run step %q", step.Name)
			for _, line := range strings.Split(cmd, "\n") {
				say(step.Name, line)
			}
			continue
		}
		announce2("step", "Running step %q", step.Name)
		err := run.Bash(ctx, cmd).Run().StreamLines(func(line string) {
			say(step.Name, line)
		})
		if err != nil {
			sayFail(step.Name, "Step failed: %v", err)
			return err
		} else {
			saySuccess("step", "Step %q succeeded", step.Name)
		}
	}
	return nil
}

func interpolate(s string, m map[string]string) string {
	for k, v := range m {
		s = strings.ReplaceAll(s, fmt.Sprintf("{{%s}}", k), v)
	}
	return s
}

func announce2(section string, format string, a ...any) {
	std.Out.WriteLine(output.Linef("👉", output.StyleBold, fmt.Sprintf("[%10s] %s", section, format), a...))
}

func say(section string, format string, a ...any) {
	sayKind(output.StyleReset, section, format, a...)
}

func sayWarn(section string, format string, a ...any) {
	sayKind(output.StyleOrange, section, format, a...)
}

func sayFail(section string, format string, a ...any) {
	sayKind(output.StyleRed, section, format, a...)
}

func saySuccess(section string, format string, a ...any) {
	sayKind(output.StyleGreen, section, format, a...)
}

func sayKind(style output.Style, section string, format string, a ...any) {
	std.Out.WriteLine(output.Linef("  ", style, fmt.Sprintf("[%10s] %s", section, format), a...))
}

func parseInputs(str string) (map[string]string, error) {
	if str == "" {
		return nil, nil
	}
	m := map[string]string{}
	parts := strings.Split(str, ",")
	for _, part := range parts {
		subparts := strings.Split(part, "=")
		if len(subparts) != 2 {
			return nil, errors.New("invalid inputs")
		}
		m[subparts[0]] = subparts[1]
	}
	return m, nil
}
