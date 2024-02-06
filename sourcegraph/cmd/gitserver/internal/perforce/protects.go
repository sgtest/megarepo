package perforce

import (
	"bytes"
	"context"
	"encoding/json"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/executil"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/gitserverfs"
	"github.com/sourcegraph/sourcegraph/internal/byteutils"
	p4types "github.com/sourcegraph/sourcegraph/internal/perforce"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"os"
)

// P4ProtectsForUser returns all protect definitions that apply to the given username.
func P4ProtectsForUser(ctx context.Context, reposDir, p4home, p4port, p4user, p4passwd, username string) ([]*p4types.Protect, error) {
	options := []P4OptionFunc{
		WithAuthentication(p4user, p4passwd),
		WithHost(p4port),
	}

	// -u User : Displays protection lines that apply to the named user. This option
	// requires super access.
	options = append(options, WithArguments("-Mj", "-ztag", "protects", "-u", username))

	scratchDir, err := gitserverfs.TempDir(reposDir, "p4-protects-")
	if err != nil {
		return nil, errors.Wrap(err, "could not create temp dir to invoke 'p4 protects'")
	}
	defer os.Remove(scratchDir)

	cmd := NewBaseCommand(ctx, p4home, scratchDir, options...)
	out, err := executil.RunCommandCombinedOutput(ctx, cmd)
	if err != nil {
		if ctxerr := ctx.Err(); ctxerr != nil {
			err = errors.Wrap(ctxerr, "p4 protects context error")
		}

		if len(out) > 0 {
			err = errors.Wrapf(err, `failed to run command "p4 protects" (output follows)\n\n%s`, specifyCommandInErrorMessage(string(out), cmd.Unwrap()))
		}

		return nil, err
	}

	if len(out) == 0 {
		// no error, but also no protects.
		return nil, nil
	}

	return parseP4Protects(out)
}

// P4ProtectsForUser returns all protect definitions that apply to the given depot.
func P4ProtectsForDepot(ctx context.Context, reposDir, p4home, p4port, p4user, p4passwd, depot string) ([]*p4types.Protect, error) {
	options := []P4OptionFunc{
		WithAuthentication(p4user, p4passwd),
		WithHost(p4port),
	}

	// -a : Displays protection lines for all users. This option requires super
	// access.
	options = append(options, WithArguments("-Mj", "-ztag", "protects", "-a", depot))

	scratchDir, err := gitserverfs.TempDir(reposDir, "p4-protects-")
	if err != nil {
		return nil, errors.Wrap(err, "could not create temp dir to invoke 'p4 protects'")
	}
	defer os.Remove(scratchDir)

	cmd := NewBaseCommand(ctx, p4home, scratchDir, options...)

	out, err := executil.RunCommandCombinedOutput(ctx, cmd)
	if err != nil {
		if ctxerr := ctx.Err(); ctxerr != nil {
			err = errors.Wrap(ctxerr, "p4 protects context error")
		}

		if len(out) > 0 {
			err = errors.Wrapf(err, `failed to run command "p4 protects" (output follows)\n\n%s`, specifyCommandInErrorMessage(string(out), cmd.Unwrap()))
		}

		return nil, err
	}

	if len(out) == 0 {
		// no error, but also no protects.
		return nil, nil
	}

	return parseP4Protects(out)
}

type perforceJSONProtect struct {
	DepotFile string  `json:"depotFile"`
	Host      string  `json:"host"`
	Line      string  `json:"line"`
	Perm      string  `json:"perm"`
	IsGroup   *string `json:"isgroup,omitempty"`
	Unmap     *string `json:"unmap,omitempty"`
	User      string  `json:"user"`
}

func parseP4Protects(out []byte) ([]*p4types.Protect, error) {
	protects := make([]*p4types.Protect, 0)

	lr := byteutils.NewLineReader(out)
	for lr.Scan() {
		line := lr.Line()

		// Trim whitespace
		line = bytes.TrimSpace(line)

		var parsedLine perforceJSONProtect
		if err := json.Unmarshal(line, &parsedLine); err != nil {
			return nil, errors.Wrap(err, "failed to unmarshal protect line")
		}

		entityType := "user"
		if parsedLine.IsGroup != nil {
			entityType = "group"
		}

		protects = append(protects, &p4types.Protect{
			Host:        parsedLine.Host,
			EntityType:  entityType,
			EntityName:  parsedLine.User,
			Match:       parsedLine.DepotFile,
			IsExclusion: parsedLine.Unmap != nil,
			Level:       parsedLine.Perm,
		})
	}

	return protects, nil
}
