package cloudsql

import (
	"fmt"

	"github.com/aws/constructs-go/constructs/v10"
	"github.com/hashicorp/terraform-cdk-go/cdktf"

	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/computenetwork"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/sqldatabase"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/sqldatabaseinstance"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/google/sqluser"
	"github.com/sourcegraph/managed-services-platform-cdktf/gen/random/password"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/random"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resource/serviceaccount"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/resourceid"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/spec"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

type Output struct {
	// Instance is the Cloud SQL database instance. It can be accessed using
	// WorkloadUser
	Instance sqldatabaseinstance.SqlDatabaseInstance
	// AdminUser is a cloudsqlsuperuser on the Cloud SQL instance.
	AdminUser sqluser.SqlUser
	// WorkloadUser is the SQL user corresponding to the Cloud Run workload
	// service account. It should be used for automatic IAM authentication:
	// https://pkg.go.dev/cloud.google.com/go/cloudsqlconn#readme-automatic-iam-database-authentication
	//
	// Before using WorkloadUser, WorkloadSuperuserGrant should be ready.
	WorkloadUser sqluser.SqlUser
}

type Config struct {
	ProjectID string
	Region    string

	// Spec configures the Cloud SQL instance.
	Spec spec.EnvironmentResourcePostgreSQLSpec
	// WorkloadIdentity is the service account attached to the Cloud Run workload.
	// A database user will be provisioned that can be accessed as this identity.
	WorkloadIdentity serviceaccount.Output
	// Network to connect the created Cloud SQL instance to.
	Network computenetwork.ComputeNetwork

	DependsOn []cdktf.ITerraformDependable
}

func New(scope constructs.Construct, id resourceid.ID, config Config) (*Output, error) {
	machineType := fmt.Sprintf("db-custom-%d-%d",
		pointers.Deref(config.Spec.CPU, 1),
		pointers.Deref(config.Spec.MemoryGB, 4)*1024)

	instance := sqldatabaseinstance.NewSqlDatabaseInstance(scope, id.TerraformID("instance"), &sqldatabaseinstance.SqlDatabaseInstanceConfig{
		Project: &config.ProjectID,
		Region:  &config.Region,

		// Current default: https://cloud.google.com/sql/docs/postgres/db-versions
		DatabaseVersion: pointers.Ptr("POSTGRES_15"),

		// Randomize instance name
		Name: pointers.Ptr(fmt.Sprintf("%s-%s",
			id.DisplayName(),
			random.New(scope, id.Group("instance_name_suffix"), random.Config{
				ByteLength: 2,
			}).HexValue)),

		Settings: &sqldatabaseinstance.SqlDatabaseInstanceSettings{
			Tier:             pointers.Ptr(machineType),
			AvailabilityType: pointers.Ptr("ZONAL"),
			DiskType:         pointers.Ptr("PD_SSD"),

			// Arbitrary starting disk size - we use autoresizing to scale the
			// disk up automatically. The minimum size is 10GB.
			DiskSize:            pointers.Float64(10),
			DiskAutoresize:      pointers.Ptr(true),
			DiskAutoresizeLimit: pointers.Float64(0),

			DatabaseFlags: []sqldatabaseinstance.SqlDatabaseInstanceSettingsDatabaseFlags{{
				Name:  pointers.Ptr("cloudsql.iam_authentication"),
				Value: pointers.Ptr("on"),
			}},

			// 🚨SECURITY🚨 SOC2/CI-79
			// Production disks for MSP are configured with daily snapshots and retention set at ninety days,
			// so we do the same.
			BackupConfiguration: &sqldatabaseinstance.SqlDatabaseInstanceSettingsBackupConfiguration{
				Enabled:                     pointers.Ptr(true),
				PointInTimeRecoveryEnabled:  pointers.Ptr(false), // PITR uses a lot of resources and is cumbersome to use
				StartTime:                   pointers.Ptr("10:00"),
				TransactionLogRetentionDays: pointers.Float64(7),
				BackupRetentionSettings: &sqldatabaseinstance.SqlDatabaseInstanceSettingsBackupConfigurationBackupRetentionSettings{
					// 🚨SECURITY🚨 SOC2/CI-79
					RetainedBackups: pointers.Float64(90),
					RetentionUnit:   pointers.Ptr("COUNT"),
				},
			},

			MaintenanceWindow: &sqldatabaseinstance.SqlDatabaseInstanceSettingsMaintenanceWindow{
				UpdateTrack: pointers.Ptr("stable"),
				Day:         pointers.Float64(1),
				Hour:        pointers.Float64(15),
			},

			InsightsConfig: &sqldatabaseinstance.SqlDatabaseInstanceSettingsInsightsConfig{
				QueryInsightsEnabled:  pointers.Ptr(true),
				QueryStringLength:     pointers.Float64(4096),
				RecordApplicationTags: pointers.Ptr(true),
				RecordClientAddress:   pointers.Ptr(true),
			},

			IpConfiguration: &sqldatabaseinstance.SqlDatabaseInstanceSettingsIpConfiguration{
				Ipv4Enabled:    pointers.Ptr(true),
				PrivateNetwork: config.Network.Id(),
				RequireSsl:     pointers.Ptr(true),
			},
		},

		// More of an inconvenience than anything else - is still delete-able
		// from the UI.
		DeletionProtection: pointers.Ptr(false),

		Lifecycle: &cdktf.TerraformResourceLifecycle{
			// Autoscaling is typically enabled - no need to worry about it
			IgnoreChanges: []string{"settings[0].disk_size"},
		},

		// Instance is the primary resource here, so placing DependsOn here
		// effectively blocks this resource from being created until dependencies
		// are ready.
		DependsOn: &config.DependsOn,
	})

	// Collect resources that must be ready before database can be accessed
	var databaseResources []cdktf.ITerraformDependable

	// Provision prerequisite databases
	for _, db := range config.Spec.Databases {
		databaseResources = append(databaseResources,
			sqldatabase.NewSqlDatabase(scope, id.Group("database").TerraformID(db),
				&sqldatabase.SqlDatabaseConfig{
					Name:     pointers.Ptr(db),
					Instance: instance.Name(),
					// PostgreSQL cannot delete databases if there are users
					// other than cloudsqlsuperuser with access
					DeletionPolicy: pointers.Ptr("ABANDON"),
				}))
	}

	// Configure a root MSP admin user
	adminPassword := password.NewPassword(scope, id.TerraformID("admin_password"), &password.PasswordConfig{
		Length:  pointers.Float64(32),
		Special: pointers.Ptr(false),
	})
	adminUser := sqluser.NewSqlUser(scope, id.TerraformID("admin_user"), &sqluser.SqlUserConfig{
		Instance: instance.Name(),
		Project:  &config.ProjectID,
		Name:     pointers.Ptr("msp-admin"),
		Password: adminPassword.Result(),

		// PostgreSQL cannot delete users with roles, so we just abandon the
		// users in a deletion event.
		DeletionPolicy: pointers.Ptr("ABANDON"),
	})

	// Grant access to workload service account
	workloadUser := sqluser.NewSqlUser(scope, id.TerraformID("workload_service_account_user"), &sqluser.SqlUserConfig{
		Instance: instance.Name(),
		Project:  &config.ProjectID,
		Type:     pointers.Ptr("CLOUD_IAM_SERVICE_ACCOUNT"),
		// Note: for Postgres only, GCP requires omitting the ".gserviceaccount.com" suffix
		// from the service account email due to length limits on database usernames.
		// See https://registry.terraform.io/providers/hashicorp/google/latest/docs/resources/sql_user
		Name: cdktf.Fn_Trimsuffix(&config.WorkloadIdentity.Email,
			pointers.Ptr(".gserviceaccount.com")),

		// PostgreSQL cannot delete users with roles, so we just abandon the
		// users in a deletion event.
		DeletionPolicy: pointers.Ptr("ABANDON"),

		// workloadUser's username is required to connect to this instance, so
		// to ensure database resources are all fully provisioned, we gate the
		// availability of this secret on all database resources being ready.
		DependsOn: &databaseResources,
	})

	return &Output{
		Instance:     instance,
		AdminUser:    adminUser,
		WorkloadUser: workloadUser,
	}, nil
}
