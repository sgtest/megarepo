package graphqlbackend

import (
	"context"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
)

// recordedCommandMaxLimit is the maximum number of recorded commands that can be
// returned in a single query. This limit prevents returning an excessive number of
// recorded commands. It should always be in sync with the default in `cmd/frontend/graphqlbackend/schema.graphql`
const recordedCommandMaxLimit = 40

var MockGetRecordedCommandMaxLimit func() int

func GetRecordedCommandMaxLimit() int {
	if MockGetRecordedCommandMaxLimit != nil {
		return MockGetRecordedCommandMaxLimit()
	}
	return recordedCommandMaxLimit
}

type RecordedCommandsArgs struct {
	Limit  int32
	Offset int32
}

func (r *RepositoryResolver) RecordedCommands(ctx context.Context, args *RecordedCommandsArgs) (graphqlutil.SliceConnectionResolver[RecordedCommandResolver], error) {
	offset := int(args.Offset)
	limit := int(args.Limit)
	maxLimit := GetRecordedCommandMaxLimit()
	if limit == 0 || limit > maxLimit {
		limit = maxLimit
	}
	currentEnd := offset + limit

	recordingConf := conf.Get().SiteConfig().GitRecorder
	if recordingConf == nil {
		return graphqlutil.NewSliceConnectionResolver([]RecordedCommandResolver{}, 0, currentEnd), nil
	}
	store := rcache.NewFIFOList(wrexec.GetFIFOListKey(r.Name()), recordingConf.Size)
	empty, err := store.IsEmpty()
	if err != nil {
		return nil, err
	}
	if empty {
		return graphqlutil.NewSliceConnectionResolver([]RecordedCommandResolver{}, 0, currentEnd), nil
	}

	// the FIFO list is zero-indexed, so we need to deduct one from the limit
	// to be able to get the correct amount of data.
	to := currentEnd - 1
	raws, err := store.Slice(ctx, offset, to)
	if err != nil {
		return nil, err
	}

	size, err := store.Size()
	if err != nil {
		return nil, err
	}

	resolvers := make([]RecordedCommandResolver, len(raws))
	for i, raw := range raws {
		command, err := wrexec.UnmarshalCommand(raw)
		if err != nil {
			return nil, err
		}
		resolvers[i] = NewRecordedCommandResolver(command)
	}

	return graphqlutil.NewSliceConnectionResolver(resolvers, size, currentEnd), nil
}

type RecordedCommandResolver interface {
	Start() gqlutil.DateTime
	Duration() float64
	Command() string
	Dir() string
	Path() string
}

type recordedCommandResolver struct {
	command wrexec.RecordedCommand
}

func NewRecordedCommandResolver(command wrexec.RecordedCommand) RecordedCommandResolver {
	return &recordedCommandResolver{command: command}
}

func (r *recordedCommandResolver) Start() gqlutil.DateTime {
	return *gqlutil.FromTime(r.command.Start)
}

func (r *recordedCommandResolver) Duration() float64 {
	return r.command.Duration
}

var urlRegex = lazyregexp.New(`((https?|ssh|git)://[^:@]+:)[^@]+(@)`)

func (r *recordedCommandResolver) Command() string {
	redacted := urlRegex.ReplaceAllString(strings.Join(r.command.Args, " "), "$1<REDACTED>$3")
	return redacted
}

func (r *recordedCommandResolver) Dir() string {
	return r.command.Dir
}

func (r *recordedCommandResolver) Path() string {
	return r.command.Path
}
