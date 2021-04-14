package resolvers

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
)

const batchChangesCredentialIDKind = "BatchChangesCredential"

const (
	siteCredentialPrefix = "site"
	userCredentialPrefix = "user"
)

func marshalBatchChangesCredentialID(id int64, isSiteCredential bool) graphql.ID {
	var idStr string
	if isSiteCredential {
		idStr = fmt.Sprintf("%s:%d", siteCredentialPrefix, id)
	} else {
		idStr = fmt.Sprintf("%s:%d", userCredentialPrefix, id)
	}
	return relay.MarshalID(batchChangesCredentialIDKind, idStr)
}

func unmarshalBatchChangesCredentialID(id graphql.ID) (credentialID int64, isSiteCredential bool, err error) {
	var strID string
	if err := relay.UnmarshalSpec(id, &strID); err != nil {
		return credentialID, isSiteCredential, err
	}

	parts := strings.SplitN(strID, ":", 2)
	if len(parts) != 2 {
		return credentialID, isSiteCredential, errors.New("invalid id")
	}

	kind := parts[0]
	switch strings.ToLower(kind) {
	case siteCredentialPrefix:
		isSiteCredential = true
	case userCredentialPrefix:
	default:
		return credentialID, isSiteCredential, fmt.Errorf("invalid id, unsupported credential kind %q", kind)
	}

	parsedID, err := strconv.Atoi(parts[1])
	return int64(parsedID), isSiteCredential, err
}

type batchChangesUserCredentialResolver struct {
	credential *database.UserCredential
}

var _ graphqlbackend.BatchChangesCredentialResolver = &batchChangesUserCredentialResolver{}

func (c *batchChangesUserCredentialResolver) ID() graphql.ID {
	return marshalBatchChangesCredentialID(c.credential.ID, false)
}

func (c *batchChangesUserCredentialResolver) ExternalServiceKind() string {
	return extsvc.TypeToKind(c.credential.ExternalServiceType)
}

func (c *batchChangesUserCredentialResolver) ExternalServiceURL() string {
	// This is usually the code host URL.
	return c.credential.ExternalServiceID
}

func (c *batchChangesUserCredentialResolver) SSHPublicKey() *string {
	if a, ok := c.credential.Credential.(auth.AuthenticatorWithSSH); ok {
		publicKey := a.SSHPublicKey()
		return &publicKey
	}
	return nil
}

func (c *batchChangesUserCredentialResolver) CreatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: c.credential.CreatedAt}
}

func (c *batchChangesUserCredentialResolver) IsSiteCredential() bool {
	return false
}

type batchChangesSiteCredentialResolver struct {
	credential *btypes.SiteCredential
}

var _ graphqlbackend.BatchChangesCredentialResolver = &batchChangesSiteCredentialResolver{}

func (c *batchChangesSiteCredentialResolver) ID() graphql.ID {
	return marshalBatchChangesCredentialID(c.credential.ID, true)
}

func (c *batchChangesSiteCredentialResolver) ExternalServiceKind() string {
	return extsvc.TypeToKind(c.credential.ExternalServiceType)
}

func (c *batchChangesSiteCredentialResolver) ExternalServiceURL() string {
	// This is usually the code host URL.
	return c.credential.ExternalServiceID
}

func (c *batchChangesSiteCredentialResolver) SSHPublicKey() *string {
	if a, ok := c.credential.Credential.(auth.AuthenticatorWithSSH); ok {
		publicKey := a.SSHPublicKey()
		return &publicKey
	}
	return nil
}

func (c *batchChangesSiteCredentialResolver) CreatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: c.credential.CreatedAt}
}

func (c *batchChangesSiteCredentialResolver) IsSiteCredential() bool {
	return true
}
