package cloudrun

import (
	"bytes"
	"html/template"
	"slices"
	"strconv"
	"strings"
	"sync"

	"golang.org/x/exp/maps"

	"github.com/hashicorp/terraform-cdk-go/cdktf"

	"github.com/sourcegraph/managed-services-platform-cdktf/gen/sentry/datasentryorganization"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/sentry/datasentryteam"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/sentry/key"
	sentryproject "github.com/sourcegraph/managed-services-platform-cdktf/gen/sentry/project"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/googlesecretsmanager"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/bigquery"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/cloudsql"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/gsmsecret"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/postgresqlroles"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/privatenetwork"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/random"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/redis"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/tfvar"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resourceid"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/cloudflareprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/dynamicvariables"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/googleprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/randomprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/sentryprovider"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/spec"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/stacks/cloudrun/internal/builder"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/stacks/cloudrun/internal/builder/job"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/stacks/cloudrun/internal/builder/service"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/stacks/iam"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

type CrossStackOutput struct {
	DiagnosticsSecret  *random.Output
	RedisInstanceID    *string
	CloudSQLInstanceID *string
	SentryProject      sentryproject.Project
}

type Variables struct {
	ProjectID string

	IAM iam.CrossStackOutput

	Service     spec.ServiceSpec
	Image       string
	Environment spec.EnvironmentSpec

	StableGenerate bool

	PreventDestroys bool
}

const StackName = "cloudrun"

const (
	OutputCloudSQLConnectionName = "cloudsql_connection_name"
)

// Hardcoded variables.
var (
	// gcpRegion is currently hardcoded.
	gcpRegion = "us-central1"
)

const tfVarKeyResolvedImageTag = "resolved_image_tag"

const SentryOrganization = "sourcegraph"

// NewStack instantiates the MSP cloudrun stack, which is currently a pretty
// monolithic stack that encompasses all the core components of an MSP service,
// including networking and dependencies like Redis.
func NewStack(stacks *stack.Set, vars Variables) (crossStackOutput *CrossStackOutput, _ error) {
	stack, locals, err := stacks.New(StackName,
		googleprovider.With(vars.ProjectID),
		cloudflareprovider.With(gsmsecret.DataConfig{
			Secret:    googlesecretsmanager.SecretCloudflareAPIToken,
			ProjectID: googlesecretsmanager.SharedSecretsProjectID,
		}),
		randomprovider.With(),
		dynamicvariables.With(vars.StableGenerate, func() (stack.TFVars, error) {
			resolvedImageTag, err := vars.Environment.Deploy.ResolveTag(vars.Image)
			return stack.TFVars{tfVarKeyResolvedImageTag: resolvedImageTag}, err
		}),
		sentryprovider.With(gsmsecret.DataConfig{
			Secret:    googlesecretsmanager.SecretSentryAuthToken,
			ProjectID: googlesecretsmanager.SharedSecretsProjectID,
		}))
	if err != nil {
		return nil, err
	}

	diagnosticsSecret := random.New(stack, resourceid.New("diagnostics-secret"), random.Config{
		ByteLength: 8,
	})

	id := resourceid.New("cloudrun")

	// Set up configuration for the Cloud Run resources
	var cloudRunBuilder builder.Builder
	switch pointers.Deref(vars.Service.Kind, spec.ServiceKindService) {
	case spec.ServiceKindService:
		cloudRunBuilder = service.NewBuilder()
	case spec.ServiceKindJob:
		cloudRunBuilder = job.NewBuilder()
	}

	// Required to enable tracing etc.
	cloudRunBuilder.AddEnv("GOOGLE_CLOUD_PROJECT", vars.ProjectID)

	// Set up secret that service should accept for diagnostics
	// endpoints.
	cloudRunBuilder.AddEnv("DIAGNOSTICS_SECRET", diagnosticsSecret.HexValue)

	// Add the domain as an environment variable.
	dnsName := pointers.DerefZero(vars.Environment.EnvironmentServiceSpec).Domain.GetDNSName()
	if dnsName != "" {
		cloudRunBuilder.AddEnv("EXTERNAL_DNS_NAME", dnsName)
	}

	// Add environment ID env var
	cloudRunBuilder.AddEnv("ENVIRONMENT_ID", vars.Environment.ID)

	// Add user-configured env vars
	if err := addContainerEnvVars(cloudRunBuilder, vars.Environment.Env, vars.Environment.SecretEnv, envVariablesData{
		ProjectID:      vars.ProjectID,
		ServiceDnsName: dnsName,
	}); err != nil {
		return nil, errors.Wrap(err, "add user env vars")
	}

	// Load image tag from tfvars.
	imageTag := tfvar.New(stack, id, tfvar.Config{
		VariableKey: tfVarKeyResolvedImageTag,
		Description: "Resolved image tag to deploy",
	})

	// privateNetworkEnabled indicates if privateNetwork has been instantiated
	// before.
	var privateNetworkEnabled bool
	// privateNetwork is only instantiated if used, and is only instantiated
	// once. If called, it always returns a non-nil value.
	privateNetwork := sync.OnceValue(func() *privatenetwork.Output {
		privateNetworkEnabled = true
		return privatenetwork.New(stack, privatenetwork.Config{
			ProjectID: vars.ProjectID,
			ServiceID: vars.Service.ID,
			Region:    gcpRegion,
		})
	})

	// Add MSP env var indicating that the service is running in a Managed
	// Services Platform environment.
	cloudRunBuilder.AddEnv("MSP", "true")

	// For SSL_CERT_DIR, configure right before final build
	sslCertDirs := []string{"/etc/ssl/certs"}

	// redisInstance is only created and non-nil if Redis is configured for the
	// environment.
	// If Redis is configured, populate cross-stack output with Redis ID.
	var redisInstanceID *string
	if vars.Environment.Resources != nil && vars.Environment.Resources.Redis != nil {
		redisInstance, err := redis.New(stack,
			resourceid.New("redis"),
			redis.Config{
				ProjectID: vars.ProjectID,
				Region:    gcpRegion,
				Spec:      *vars.Environment.Resources.Redis,
				Network:   privateNetwork().Network,
			})
		if err != nil {
			return nil, errors.Wrap(err, "failed to render Redis instance")
		}

		redisInstanceID = redisInstance.ID

		// Configure endpoint string.
		cloudRunBuilder.AddEnv("REDIS_ENDPOINT", redisInstance.Endpoint)

		// Mount the custom cert and add it to SSL_CERT_DIR
		caCertVolumeName := "redis-ca-cert"
		cloudRunBuilder.AddSecretVolume(
			caCertVolumeName,
			"redis-ca-cert.pem",
			builder.SecretRef{
				Name:    redisInstance.Certificate.ID,
				Version: redisInstance.Certificate.Version,
			},
			292, // 0444 read-only
		)
		cloudRunBuilder.AddVolumeMount(caCertVolumeName, "/etc/ssl/custom-certs")
		sslCertDirs = append(sslCertDirs, "/etc/ssl/custom-certs")
	}

	var cloudSQLInstanceID *string
	if vars.Environment.Resources != nil && vars.Environment.Resources.PostgreSQL != nil {
		pgSpec := *vars.Environment.Resources.PostgreSQL
		sqlInstance, err := cloudsql.New(stack, resourceid.New("postgresql"), cloudsql.Config{
			ProjectID: vars.ProjectID,
			Region:    gcpRegion,
			Spec:      pgSpec,
			Network:   privateNetwork().Network,

			WorkloadIdentity:       *vars.IAM.CloudRunWorkloadServiceAccount,
			OperatorAccessIdentity: *vars.IAM.OperatorAccessServiceAccount,

			PreventDestroys: vars.PreventDestroys,

			// ServiceNetworkingConnection is required for Cloud SQL to connect
			// to the private network, so we must wait for it to be provisioned.
			// See https://cloud.google.com/sql/docs/mysql/private-ip#network_requirements
			DependsOn: []cdktf.ITerraformDependable{
				privateNetwork().ServiceNetworkingConnection,
			},
		})
		if err != nil {
			return nil, errors.Wrap(err, "failed to render Cloud SQL instance")
		}

		cloudSQLInstanceID = sqlInstance.Instance.Id()

		// Add parameters required for authentication
		cloudRunBuilder.AddEnv("PGINSTANCE", *sqlInstance.Instance.ConnectionName())
		cloudRunBuilder.AddEnv("PGUSER", *sqlInstance.WorkloadUser.Name())
		// NOTE: https://pkg.go.dev/cloud.google.com/go/cloudsqlconn#section-readme
		// magically handles certs for us, so we don't need to mount certs in
		// Cloud Run.

		// Apply additional runtime configuration
		pgRoles, err := postgresqlroles.New(stack, id.Group("postgresqlroles"), postgresqlroles.Config{
			Databases: pgSpec.Databases,
			CloudSQL:  sqlInstance,
		})
		if err != nil {
			return nil, errors.Wrap(err, "failed to render Cloud SQL PostgreSQL roles")
		}

		// We need the workload superuser role to be granted before Cloud Run
		// can correctly use the database instance
		cloudRunBuilder.AddDependency(pgRoles.WorkloadSuperuserGrant)

		// Add output for connecting to the instance
		locals.Add("cloudsql_connection_name", *sqlInstance.Instance.ConnectionName(),
			"Cloud SQL database connection name")
	}

	// bigqueryDataset is only created and non-nil if BigQuery is configured for
	// the environment.
	if vars.Environment.Resources != nil && vars.Environment.Resources.BigQueryDataset != nil {
		bigqueryDataset, err := bigquery.New(stack, resourceid.New("bigquery"), bigquery.Config{
			DefaultProjectID:       vars.ProjectID,
			ServiceID:              vars.Service.ID,
			WorkloadServiceAccount: vars.IAM.CloudRunWorkloadServiceAccount,
			Spec:                   *vars.Environment.Resources.BigQueryDataset,
			PreventDestroys:        vars.PreventDestroys,
		})
		if err != nil {
			return nil, errors.Wrap(err, "failed to render BigQuery dataset")
		}

		// Add parameters required for writing to the correct BigQuery dataset
		cloudRunBuilder.AddEnv("BIGQUERY_PROJECT_ID", bigqueryDataset.ProjectID)
		cloudRunBuilder.AddEnv("BIGQUERY_DATASET_ID", bigqueryDataset.DatasetID)

		// Make sure tables are available before Cloud Run
		for _, t := range bigqueryDataset.Tables {
			cloudRunBuilder.AddDependency(t)
		}
	}

	// Sentry
	var sentryProject sentryproject.Project
	{
		id := id.Group("sentry")
		// Get the Sentry organization
		organization := datasentryorganization.NewDataSentryOrganization(stack, id.TerraformID("organization"), &datasentryorganization.DataSentryOrganizationConfig{
			Slug: pointers.Ptr(SentryOrganization),
		})

		// Get the Sourcegraph team - we don't use individual owner teams
		// because it's hard to tell whether they already exist or not, and
		// it's not really important enough to force operators to create a
		// team by hand. We depend on Opsgenie teams for concrete ownership
		// instead.
		sentryTeam := datasentryteam.NewDataSentryTeam(stack, id.TerraformID("team"), &datasentryteam.DataSentryTeamConfig{
			Organization: organization.Id(),
			Slug:         pointers.Ptr("sourcegraph"),
		})

		// Create the project
		sentryProject = sentryproject.NewProject(stack, id.TerraformID("project"), &sentryproject.ProjectConfig{
			Organization: organization.Id(),
			Name:         pointers.Stringf("%s - %s", vars.Service.GetName(), vars.Environment.ID),
			Slug:         pointers.Stringf("%s-%s", vars.Service.ID, vars.Environment.ID),
			Teams:        &[]*string{sentryTeam.Slug()},
			DefaultRules: pointers.Ptr(false),
		})

		// Create a DSN
		key := key.NewKey(stack, id.TerraformID("dsn"), &key.KeyConfig{
			Organization: organization.Id(),
			Project:      sentryProject.Slug(),
			Name:         pointers.Ptr("Managed Servcies Platform"),
		})

		cloudRunBuilder.AddEnv("SENTRY_DSN", *key.DsnPublic())
	}

	// Finalize output of builder
	cloudRunBuilder.AddEnv("SSL_CERT_DIR", strings.Join(sslCertDirs, ":"))
	cloudRunResource, err := cloudRunBuilder.Build(stack, builder.Variables{
		Service:           vars.Service,
		Image:             vars.Image,
		ResolvedImageTag:  *imageTag.StringValue,
		Environment:       vars.Environment,
		GCPProjectID:      vars.ProjectID,
		GCPRegion:         gcpRegion,
		ServiceAccount:    vars.IAM.CloudRunWorkloadServiceAccount,
		DiagnosticsSecret: diagnosticsSecret,
		ResourceLimits:    makeContainerResourceLimits(vars.Environment.Instances.Resources),
		PrivateNetwork: func() *privatenetwork.Output {
			if privateNetworkEnabled {
				return privateNetwork()
			}
			return nil
		}(),
	})
	if err != nil {
		return nil, errors.Wrapf(err, "build Cloud Run resource kind %q", cloudRunBuilder.Kind())
	}

	// Collect outputs
	locals.Add("cloud_run_resource_name", *cloudRunResource.Name(),
		"Cloud Run resource name")
	locals.Add("cloud_run_location", *cloudRunResource.Location(),
		"Cloud Run resource location")
	locals.Add("image_tag", *imageTag.StringValue,
		"Resolved tag of service image to deploy")
	return &CrossStackOutput{
		DiagnosticsSecret:  diagnosticsSecret,
		RedisInstanceID:    redisInstanceID,
		CloudSQLInstanceID: cloudSQLInstanceID,
		SentryProject:      sentryProject,
	}, nil
}

type envVariablesData struct {
	ProjectID      string
	ServiceDnsName string
}

func addContainerEnvVars(
	b builder.Builder,
	env map[string]string,
	secretEnv map[string]string,
	varsData envVariablesData,
) error {
	// Apply static env vars
	envKeys := maps.Keys(env)
	slices.Sort(envKeys)
	for _, k := range envKeys {
		tmpl, err := template.New("").Parse(env[k])
		if err != nil {
			return errors.Wrapf(err, "parse env var template: %q", env[k])
		}
		var buf bytes.Buffer
		if err = tmpl.Execute(&buf, varsData); err != nil {
			return errors.Wrapf(err, "execute template: %q", env[k])
		}

		b.AddEnv(k, buf.String())
	}

	// Apply secret env vars
	secretEnvKeys := maps.Keys(secretEnv)
	slices.Sort(secretEnvKeys)
	for _, k := range secretEnvKeys {
		b.AddSecretEnv(k, builder.SecretRef{
			Name:    secretEnv[k],
			Version: "latest",
		})
	}

	return nil
}

func makeContainerResourceLimits(r spec.EnvironmentInstancesResourcesSpec) map[string]*string {
	return map[string]*string{
		"cpu":    pointers.Ptr(strconv.Itoa(r.CPU)),
		"memory": pointers.Ptr(r.Memory),
	}
}
