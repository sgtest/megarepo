package gitdomain

import (
	"context"
	"encoding/hex"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type GetObjectFunc func(ctx context.Context, repo api.RepoName, objectName string) (*GitObject, error)

// GetObjectService will get an information about a git object
// TODO: Do we really need a service? Could we not just have a function that returns a GetObjectFunc given
// RevParse and GetObjectType funcs?
type GetObjectService struct {
	RevParse      func(ctx context.Context, repo api.RepoName, rev string) (string, error)
	GetObjectType func(ctx context.Context, repo api.RepoName, objectID string) (ObjectType, error)
}

func (s *GetObjectService) GetObject(ctx context.Context, repo api.RepoName, objectName string) (*GitObject, error) {
	if err := checkSpecArgSafety(objectName); err != nil {
		return nil, err
	}

	sha, err := s.RevParse(ctx, repo, objectName)
	if err != nil {
		if IsRepoNotExist(err) {
			return nil, err
		}
		if strings.Contains(sha, "unknown revision") {
			return nil, &RevisionNotFoundError{Repo: repo, Spec: objectName}
		}
		return nil, err
	}

	sha = strings.TrimSpace(sha)
	if !IsAbsoluteRevision(sha) {
		if sha == "HEAD" {
			// We don't verify the existence of HEAD, but if HEAD doesn't point to anything
			// git just returns `HEAD` as the output of rev-parse. An example where this
			// occurs is an empty repository.
			return nil, &RevisionNotFoundError{Repo: repo, Spec: objectName}
		}
		return nil, &BadCommitError{Spec: objectName, Commit: api.CommitID(sha), Repo: repo}
	}

	oid, err := decodeOID(sha)
	if err != nil {
		return nil, errors.Wrap(err, "decoding oid")
	}

	objectType, err := s.GetObjectType(ctx, repo, oid.String())
	if err != nil {
		return nil, errors.Wrap(err, "getting object type")
	}

	return &GitObject{
		ID:   oid,
		Type: objectType,
	}, nil
}

// checkSpecArgSafety returns a non-nil err if spec begins with a "-", which could
// cause it to be interpreted as a git command line argument.
func checkSpecArgSafety(spec string) error {
	if strings.HasPrefix(spec, "-") {
		return errors.Errorf("invalid git revision spec %q (begins with '-')", spec)
	}
	return nil
}

func decodeOID(sha string) (OID, error) {
	oidBytes, err := hex.DecodeString(sha)
	if err != nil {
		return OID{}, err
	}
	var oid OID
	copy(oid[:], oidBytes)
	return oid, nil
}
