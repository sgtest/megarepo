package search

import (
	"context"
	"math"
	"strings"
	"sync"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/sourcegraph/sourcegraph/internal/searcher/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/searcher/v1"
)

type Server struct {
	Service *Service
	proto.UnimplementedSearcherServiceServer
}

func (s *Server) Search(req *proto.SearchRequest, stream proto.SearcherService_SearchServer) error {
	var p protocol.Request
	p.FromProto(req)

	if !p.PatternMatchesContent && !p.PatternMatchesPath {
		// BACKCOMPAT: Old frontends send neither of these fields, but we still want to
		// search file content in that case.
		p.PatternMatchesContent = true
	}
	if err := validateParams(&p); err != nil {
		return status.Error(codes.InvalidArgument, err.Error())
	}

	if p.Limit == 0 {
		// No limit for streaming search since upstream limits
		// will either be sent in the request, or propagated by
		// a cancelled context.
		p.Limit = math.MaxInt32
	}

	// mu protects the stream from concurrent writes.
	var mu sync.Mutex
	onMatches := func(match protocol.FileMatch) {
		mu.Lock()
		defer mu.Unlock()

		stream.Send(&proto.SearchResponse{
			Message: &proto.SearchResponse_FileMatch{
				FileMatch: match.ToProto(),
			},
		})
	}

	ctx, cancel, matchStream := newLimitedStream(stream.Context(), int(p.PatternInfo.Limit), onMatches)
	defer cancel()

	err := s.Service.search(ctx, &p, matchStream)
	if err != nil {
		return convertToGRPCError(ctx, err)
	}

	return stream.Send(&proto.SearchResponse{
		Message: &proto.SearchResponse_DoneMessage{
			DoneMessage: &proto.SearchResponse_Done{
				LimitHit: matchStream.LimitHit(),
			},
		},
	})
}

// convertToGRPCError converts an error into a gRPC status error code.
//
// If err is nil, it returns nil.
//
// If err is already a gRPC status error, it is returned as-is.
//
// If the provided context has expired, a grpc codes.Canceled / DeadlineExceeded error is returned.
//
// If the err is a well-known error (such as a process getting killed, etc.),
// it's mapped to the appropriate gRPC status code.
//
// Otherwise, err is converted to an Unknown gRPC error code.
func convertToGRPCError(ctx context.Context, err error) error {
	if err == nil {
		return nil
	}

	// don't convert an existing status error
	if statusErr, ok := status.FromError(err); ok {
		return statusErr.Err()
	}

	// if the context expired, just return that
	if ctxErr := ctx.Err(); ctxErr != nil {
		return status.FromContextError(ctxErr).Err()
	}

	// otherwise convert to a status error
	grpcCode := codes.Unknown
	if strings.Contains(err.Error(), "signal: killed") {
		grpcCode = codes.Aborted
	}

	return status.New(grpcCode, err.Error()).Err()
}
