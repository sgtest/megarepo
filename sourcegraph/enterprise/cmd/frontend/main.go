// Command frontend is the enterprise frontend program.
package main

import (
	"os"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/shared"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
	"github.com/sourcegraph/sourcegraph/internal/sanitycheck"
	"github.com/sourcegraph/sourcegraph/internal/service/svcmain"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
	"github.com/sourcegraph/sourcegraph/ui/assets"

	_ "github.com/sourcegraph/sourcegraph/ui/assets/enterprise" // Select enterprise assets
)

func init() {
	// TODO(sqs): TODO(single-binary): could we move this out of init?
	oobmigration.ReturnEnterpriseMigrations = true
}

func main() {
	sanitycheck.Pass()
	if os.Getenv("WEBPACK_DEV_SERVER") == "1" {
		assets.UseDevAssetsProvider()
	}
	svcmain.SingleServiceMainWithoutConf(shared.Service, svcmain.Config{}, svcmain.OutOfBandConfiguration{
		// use a switchable config here so we can switch it out for a proper conf client
		// once we can use it after autoupgrading
		Logging: conf.NewLogsSinksSource(shared.SwitchableSiteConfig()),
		Tracing: tracer.ConfConfigurationSource{WatchableSiteConfig: shared.SwitchableSiteConfig()},
	})
}
