package cloudrun

import (
	"fmt"
	"strings"

	"github.com/hashicorp/terraform-cdk-go/cdktf"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/cloudrunv2service"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/googlesecretsmanager"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/bigquery"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/cloudflare"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/gsmsecret"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/loadbalancer"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/random"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/redis"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/serviceaccount"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resourceid"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/cloudflareprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/googleprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/randomprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/spec"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

type Output struct{}

type Variables struct {
	ProjectID string

	Service     spec.ServiceSpec
	Image       string
	Environment spec.EnvironmentSpec
}

const StackName = "cloudrun"

// Hardcoded variables.
var (
	// gcpRegion is currently hardcoded.
	gcpRegion = "us-central1"
	// serviceAccountRoles are granted to the service account for the Cloud Run service.
	serviceAccountRoles = []serviceaccount.Role{
		// Allow env vars to source from secrets
		{ID: "role_secret_accessor", Role: "roles/secretmanager.secretAccessor"},
		// Allow service to access private networks
		{ID: "role_compute_networkuser", Role: "roles/compute.networkUser"},
		// Allow service to emit observability
		{ID: "role_cloudtrace_agent", Role: "roles/cloudtrace.agent"},
		{ID: "role_monitoring_metricwriter", Role: "roles/monitoring.metricWriter"},
	}
	// servicePort is provided to the container as $PORT in Cloud Run:
	// https://cloud.google.com/run/docs/configuring/services/containers#configure-port
	servicePort = 9992
	// healthCheckEndpoint is the default healthcheck endpoint for all services.
	healthCheckEndpoint = "/-/healthz"
)

// Default values.
var (
	// defaultMaxInstances is the default Scaling.MaxCount
	defaultMaxInstances = 5
	// defaultMaxConcurrentRequests is the default scaling.MaxRequestConcurrency
	// It is set very high to prefer fewer instances, as Go services can generally
	// handle very high load without issue.
	defaultMaxConcurrentRequests = 1000
)

// makeServiceEnvVarPrefix returns the env var prefix for service-specific
// env vars that will be set on the Cloud Run service, i.e.
//
// - ${local.env_var_prefix}_BIGQUERY_PROJECT_ID
// - ${local.env_var_prefix}_BIGQUERY_DATASET
// - ${local.env_var_prefix}_BIGQUERY_TABLE
// - ${local.env_var_prefix}_DIAGNOSTICS_SECRET
//
// The prefix is an all-uppercase underscore-delimited version of the service ID,
// for example:
//
//	cody-gateway
//
// The prefix for various env vars will be:
//
//	CODY_GATEWAY_
//
// Note that some variables like GOOGLE_PROJECT_ID and REDIS_ENDPOINT do not
// get prefixed, and custom env vars configured on an environment are not prefixed
// either.
func makeServiceEnvVarPrefix(serviceID string) string {
	return strings.ToUpper(strings.ReplaceAll(serviceID, "-", "_")) + "_"
}

// NewStack instantiates the MSP cloudrun stack, which is currently a pretty
// monolithic stack that encompasses all the core components of an MSP service,
// including networking and dependencies like Redis.
func NewStack(stacks *stack.Set, vars Variables) (*Output, error) {
	stack := stacks.New(StackName,
		googleprovider.With(vars.ProjectID),
		cloudflareprovider.With(gsmsecret.DataConfig{
			Secret:    googlesecretsmanager.SecretCloudflareAPIToken,
			ProjectID: googlesecretsmanager.ProjectID,
		}),
		randomprovider.With())

	// Set up a service-specific env var prefix to avoid conflicts where relevant
	serviceEnvVarPrefix := pointers.Deref(
		vars.Service.EnvVarPrefix,
		makeServiceEnvVarPrefix(vars.Service.ID))

	diagnosticsSecret := random.New(stack, resourceid.New("diagnostics-secret"), random.Config{
		ByteLength: 8,
	})

	// Set up configuration for the core Cloud Run service
	cloudRun := &cloudRunServiceBuilder{
		ServiceAccount: serviceaccount.New(stack, resourceid.New("cloudrun"), serviceaccount.Config{
			ProjectID: vars.ProjectID,
			AccountID: vars.Service.ID,
			DisplayName: fmt.Sprintf("%s Service Account",
				pointers.Deref(vars.Service.Name, vars.Service.ID)),
			Roles: serviceAccountRoles,
		}),

		DiagnosticsSecret: diagnosticsSecret,
		// Set up some base env vars
		AdditionalEnv: []*cloudrunv2service.CloudRunV2ServiceTemplateContainersEnv{
			{
				// Required to enable tracing etc.
				//
				// We don't use serviceEnvVarPrefix here because this is a
				// convention to indicate the environment's project.
				Name:  pointers.Ptr("GOOGLE_CLOUD_PROJECT"),
				Value: &vars.ProjectID,
			},
			{
				// Set up secret that service should accept for diagnostics
				// endpoints
				Name:  pointers.Ptr(serviceEnvVarPrefix + "DIAGNOSTICS_SECRET"),
				Value: &diagnosticsSecret.HexValue,
			},
		},
	}
	if vars.Environment.Resources.NeedsCloudRunConnector() {
		cloudRun.PrivateNetwork = newCloudRunPrivateNetwork(stack, cloudRunPrivateNetworkConfig{
			ProjectID: vars.ProjectID,
			ServiceID: vars.Service.ID,
			Region:    gcpRegion,
		})
	}

	// redisInstance is only created and non-nil if Redis is configured for the
	// environment.
	if vars.Environment.Resources != nil && vars.Environment.Resources.Redis != nil {
		redisInstance, err := redis.New(stack,
			resourceid.New("redis"),
			redis.Config{
				ProjectID: vars.ProjectID,
				Network:   cloudRun.PrivateNetwork.network,
				Region:    gcpRegion,
				Spec:      *vars.Environment.Resources.Redis,
			})
		if err != nil {
			return nil, errors.Wrap(err, "failed to render Redis instance")
		}
		cloudRun.AdditionalEnv = append(cloudRun.AdditionalEnv,
			&cloudrunv2service.CloudRunV2ServiceTemplateContainersEnv{
				// We don't use serviceEnvVarPrefix here because this is a
				// Sourcegraph-wide convention.
				Name:  pointers.Ptr("REDIS_ENDPOINT"),
				Value: pointers.Ptr(redisInstance.Endpoint),
			})

		caCertVolumeName := "redis-ca-cert"
		cloudRun.AdditionalVolumes = append(cloudRun.AdditionalVolumes,
			&cloudrunv2service.CloudRunV2ServiceTemplateVolumes{
				Name: pointers.Ptr(caCertVolumeName),
				Secret: &cloudrunv2service.CloudRunV2ServiceTemplateVolumesSecret{
					Secret: &redisInstance.Certificate.ID,
					Items: []*cloudrunv2service.CloudRunV2ServiceTemplateVolumesSecretItems{{
						Version: &redisInstance.Certificate.Version,
						Path:    pointers.Ptr("redis-ca-cert.pem"),
						Mode:    pointers.Float64(292), // 0444 read-only
					}},
				},
			})
		cloudRun.AdditionalVolumeMounts = append(cloudRun.AdditionalVolumeMounts,
			&cloudrunv2service.CloudRunV2ServiceTemplateContainersVolumeMounts{
				Name: pointers.Ptr(caCertVolumeName),
				// TODO: Use subpath if google_cloud_run_v2_service adds support for it:
				// https://registry.terraform.io/providers/hashicorp/google-beta/latest/docs/resources/cloud_run_v2_service#mount_path
				MountPath: pointers.Ptr("/etc/ssl/custom-certs"),
			})
	}

	// bigqueryDataset is only created and non-nil if BigQuery is configured for
	// the environment.
	if vars.Environment.Resources != nil && vars.Environment.Resources.BigQueryTable != nil {
		bigqueryDataset, err := bigquery.New(stack, resourceid.New("bigquery"), bigquery.Config{
			DefaultProjectID: vars.ProjectID,
			Spec:             *vars.Environment.Resources.BigQueryTable,
		})
		if err != nil {
			return nil, errors.Wrap(err, "failed to render BigQuery dataset")
		}
		cloudRun.AdditionalEnv = append(cloudRun.AdditionalEnv,
			&cloudrunv2service.CloudRunV2ServiceTemplateContainersEnv{
				Name:  pointers.Ptr(serviceEnvVarPrefix + "BIGQUERY_PROJECT_ID"),
				Value: pointers.Ptr(bigqueryDataset.ProjectID),
			}, &cloudrunv2service.CloudRunV2ServiceTemplateContainersEnv{
				Name:  pointers.Ptr(serviceEnvVarPrefix + "BIGQUERY_DATASET"),
				Value: pointers.Ptr(bigqueryDataset.Dataset),
			}, &cloudrunv2service.CloudRunV2ServiceTemplateContainersEnv{
				Name:  pointers.Ptr(serviceEnvVarPrefix + "BIGQUERY_TABLE"),
				Value: pointers.Ptr(bigqueryDataset.Table),
			})
	}

	// Finally, create the Cloud Run service with the finalized service
	// configuration
	service, err := cloudRun.Build(stack, vars)
	if err != nil {
		return nil, err
	}

	// Now we add a load balancer
	lb := loadbalancer.New(stack, resourceid.New("lb-backend"), loadbalancer.Config{
		ProjectID:     vars.ProjectID,
		Region:        gcpRegion,
		TargetService: service,
	})

	// Then whatever the user requested to expose the service publicly
	switch vars.Environment.Domain.Type {
	case "cloudflare":
		if _, err := cloudflare.New(stack, resourceid.New("cf"), cloudflare.Config{
			Spec:   *vars.Environment.Domain.Cloudflare,
			Target: *lb,
		}); err != nil {
			return nil, err
		}
	}

	return &Output{}, nil
}

// cloudRunServiceBuilder parameterizes configurable components of the core
// Cloud Run Service. It's particularly useful for strongly typing fields that
// the generated CDKTF library accepts as interface{} types.
type cloudRunServiceBuilder struct {
	// ServiceAccount for the Cloud Run instance
	ServiceAccount *serviceaccount.Output
	// DiagnosticsSecret is the secret for healthcheck endpoints
	DiagnosticsSecret *random.Output
	// PrivateNetwork is configured if required as an Iinternal network for the
	// Cloud Run service to talk to other GCP resources.
	PrivateNetwork *cloudRunPrivateNetworkOutput

	AdditionalEnv          []*cloudrunv2service.CloudRunV2ServiceTemplateContainersEnv
	AdditionalVolumes      []*cloudrunv2service.CloudRunV2ServiceTemplateVolumes
	AdditionalVolumeMounts []*cloudrunv2service.CloudRunV2ServiceTemplateContainersVolumeMounts
}

func (c cloudRunServiceBuilder) Build(stack cdktf.TerraformStack, vars Variables) (cloudrunv2service.CloudRunV2Service, error) {
	// TODO Make this fancier, for now this is just a sketch of maybe CD?
	serviceImageTag, err := vars.Environment.Deploy.ResolveTag()
	if err != nil {
		return nil, err
	}

	var vpcAccess *cloudrunv2service.CloudRunV2ServiceTemplateVpcAccess
	if c.PrivateNetwork != nil {
		vpcAccess = &cloudrunv2service.CloudRunV2ServiceTemplateVpcAccess{
			Connector: c.PrivateNetwork.connector.SelfLink(),
			Egress:    pointers.Ptr("PRIVATE_RANGES_ONLY"),
		}
	}

	return cloudrunv2service.NewCloudRunV2Service(stack, pointers.Ptr("cloudrun"), &cloudrunv2service.CloudRunV2ServiceConfig{
		Name:     pointers.Ptr(vars.Service.ID),
		Location: pointers.Ptr(gcpRegion),

		//  Disallows direct traffic from public internet, we have a LB set up for that.
		Ingress: pointers.Ptr("INGRESS_TRAFFIC_INTERNAL_LOAD_BALANCER"),

		Template: &cloudrunv2service.CloudRunV2ServiceTemplate{
			// Act under our provisioned service account
			ServiceAccount: pointers.Ptr(c.ServiceAccount.Email),

			// Connect to VPC connector for talking to other GCP services.
			VpcAccess: vpcAccess,

			// Set a high limit that matches our default Cloudflare zone's
			// timeout:
			//
			//   export CF_API_TOKEN=$(gcloud secrets versions access latest --secret CLOUDFLARE_API_TOKEN --project sourcegraph-secrets)
			//   curl -H "Authorization: Bearer $CF_API_TOKEN" https://api.cloudflare.com/client/v4/zones | jq '.result[]  | select(.name == "sourcegraph.com") | .id'
			//   curl -H "Authorization: Bearer $CF_API_TOKEN" https://api.cloudflare.com/client/v4/zones/$CF_ZONE_ID/settings | jq '.result[] | select(.id == "proxy_read_timeout")'
			//
			// Result should be something like:
			//
			//   {
			//     "id": "proxy_read_timeout",
			//     "value": "300",
			//     "modified_on": "2022-02-08T23:10:35.772888Z",
			//     "editable": true
			//   }
			//
			// The service should implement tighter timeouts on its own if desired.
			Timeout: pointers.Ptr("300s"),

			// Scaling configuration
			MaxInstanceRequestConcurrency: pointers.Float64(
				pointers.Deref(vars.Environment.Instances.Scaling.MaxRequestConcurrency, defaultMaxConcurrentRequests)),
			Scaling: &cloudrunv2service.CloudRunV2ServiceTemplateScaling{
				MinInstanceCount: pointers.Float64(vars.Environment.Instances.Scaling.MinCount),
				MaxInstanceCount: pointers.Float64(
					pointers.Deref(vars.Environment.Instances.Scaling.MaxCount, defaultMaxInstances)),
			},

			// Configuration for the single service container.
			Containers: []*cloudrunv2service.CloudRunV2ServiceTemplateContainers{{
				Name:  pointers.Ptr(vars.Service.ID),
				Image: pointers.Ptr(fmt.Sprintf("%s:%s", vars.Image, serviceImageTag)),

				Resources: &cloudrunv2service.CloudRunV2ServiceTemplateContainersResources{
					Limits: makeContainerResourceLimits(vars.Environment.Instances.Resources),
				},

				Ports: []*cloudrunv2service.CloudRunV2ServiceTemplateContainersPorts{{
					// ContainerPort is provided to the container as $PORT in Cloud Run
					ContainerPort: pointers.Float64(servicePort),
				}},

				Env: append(
					makeContainerEnvVars(
						vars.Environment.Env,
						vars.Environment.SecretEnv,
					),
					c.AdditionalEnv...),

				// Do healthchecks with authorization based on MSP convention.
				StartupProbe: &cloudrunv2service.CloudRunV2ServiceTemplateContainersStartupProbe{
					HttpGet: &cloudrunv2service.CloudRunV2ServiceTemplateContainersStartupProbeHttpGet{
						Path: pointers.Ptr(healthCheckEndpoint),
						HttpHeaders: []*cloudrunv2service.CloudRunV2ServiceTemplateContainersStartupProbeHttpGetHttpHeaders{{
							Name:  pointers.Ptr("Bearer"),
							Value: pointers.Ptr(fmt.Sprintf("Authorization %s", c.DiagnosticsSecret.HexValue)),
						}},
					},
					InitialDelaySeconds: pointers.Float64(0),
					TimeoutSeconds:      pointers.Float64(1),
					PeriodSeconds:       pointers.Float64(2),
					FailureThreshold:    pointers.Float64(3),
				},

				LivenessProbe: func() *cloudrunv2service.CloudRunV2ServiceTemplateContainersLivenessProbe {
					if vars.Environment.Healthcheck == nil || vars.Environment.Healthcheck.LivenessProbeInterval == nil {
						return nil
					}
					return &cloudrunv2service.CloudRunV2ServiceTemplateContainersLivenessProbe{
						HttpGet: &cloudrunv2service.CloudRunV2ServiceTemplateContainersLivenessProbeHttpGet{
							Path: pointers.Ptr(healthCheckEndpoint),
							HttpHeaders: []*cloudrunv2service.CloudRunV2ServiceTemplateContainersLivenessProbeHttpGetHttpHeaders{{
								Name:  pointers.Ptr("Bearer"),
								Value: pointers.Ptr(fmt.Sprintf("Authorization %s", c.DiagnosticsSecret.HexValue)),
							}},
						},
						TimeoutSeconds:   pointers.Float64(3),
						PeriodSeconds:    pointers.Float64(*vars.Environment.Healthcheck.LivenessProbeInterval),
						FailureThreshold: pointers.Float64(2),
					}
				}(),

				VolumeMounts: c.AdditionalVolumeMounts,
			}},

			Volumes: c.AdditionalVolumes,
		}}), nil
}
