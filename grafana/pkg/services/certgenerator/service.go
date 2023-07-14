package certgenerator

import (
	"path/filepath"

	"github.com/grafana/grafana/pkg/modules"
	"github.com/grafana/grafana/pkg/setting"

	"github.com/grafana/grafana-apiserver/pkg/certgenerator"
)

func ProvideService(cfg *setting.Cfg) (*certgenerator.Service, error) {
	return certgenerator.CreateService(modules.CertGenerator, filepath.Join(cfg.DataPath, "k8s"))
}
