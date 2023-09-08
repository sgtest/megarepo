package redis

import (
	"fmt"
	"strings"

	"github.com/aws/constructs-go/constructs/v10"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/computenetwork"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/redisinstance"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/gsmsecret"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resourceid"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/spec"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

type Output struct {
	Endpoint    string
	Certificate gsmsecret.Output
}

type Config struct {
	ProjectID string

	Region  string
	Network computenetwork.ComputeNetwork

	Spec spec.EnvironmentResourceRedisSpec
}

// TODO: Add validation
func New(scope constructs.Construct, id resourceid.ID, config Config) (*Output, error) {
	redis := redisinstance.NewRedisInstance(scope,
		id.ResourceID("instance"),
		&redisinstance.RedisInstanceConfig{
			Project: pointers.Ptr(config.ProjectID),
			Region:  &config.Region,
			Name:    pointers.Ptr(id.DisplayName()),

			Tier:         pointers.Ptr(pointers.Deref(config.Spec.Tier, "STANDARD_HA")),
			MemorySizeGb: pointers.Float64(pointers.Deref(config.Spec.MemoryGB, 1)),

			AuthEnabled:           true,
			TransitEncryptionMode: pointers.Ptr("SERVER_AUTHENTICATION"),
			PersistenceConfig: &redisinstance.RedisInstancePersistenceConfig{
				PersistenceMode: pointers.Ptr("RDB"),
			},

			AuthorizedNetwork: config.Network.SelfLink(),
		})

	// Share CA certificate for connecting to Redis over TLS as a GSM secret
	redisCACert := gsmsecret.New(scope, id.SubID("ca-cert"), gsmsecret.Config{
		ProjectID: config.ProjectID,
		ID:        strings.ToUpper(id.DisplayName()) + "_CA_CERT",
		Value:     *redis.ServerCaCerts().Get(pointers.Float64(0)).Cert(),
	})

	return &Output{
		// Note double-s "rediss" for TLS
		// https://registry.terraform.io/providers/hashicorp/google/latest/docs/resources/redis_instance#server_ca_certs
		Endpoint: fmt.Sprintf("rediss://:%s@%s:%d",
			*redis.AuthString(), *redis.Host(), int(*redis.Port())),
		Certificate: *redisCACert,
	}, nil
}
