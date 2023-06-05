package run

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path"
	"runtime"
	"strings"

	"github.com/coreos/go-iptables/iptables"
	"github.com/sourcegraph/log"
	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/executor/internal/apiclient"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/executor/internal/apiclient/queue"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/executor/internal/config"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/executor/internal/util"
	"github.com/sourcegraph/sourcegraph/internal/download"
	srccli "github.com/sourcegraph/sourcegraph/internal/src-cli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func InstallIgnite(cliCtx *cli.Context, runner util.CmdRunner, logger log.Logger, config *config.Config) error {
	if !hostMightBeAbleToRunIgnite() {
		return ErrNoIgniteSupport
	}

	return installIgnite(cliCtx)
}

func InstallCNI(cliCtx *cli.Context, runner util.CmdRunner, logger log.Logger, cfg *config.Config) error {
	if !hostMightBeAbleToRunIgnite() {
		return ErrNoIgniteSupport
	}

	return installCNIPlugins(cliCtx)
}

func InstallSrc(cliCtx *cli.Context, _ util.CmdRunner, logger log.Logger, config *config.Config) error {
	return installSrc(cliCtx, logger, config)
}

func InstallIPTablesRules(cliCtx *cli.Context, runner util.CmdRunner, logger log.Logger, config *config.Config) error {
	if !hostMightBeAbleToRunIgnite() {
		return ErrNoIgniteSupport
	}

	recreateChain := cliCtx.Bool("recreate-chain")
	if !recreateChain {
		logger.Info("Creating iptables entries for CNI_ADMIN chain if not present")
	} else {
		logger.Info("Recreating iptables entries for CNI_ADMIN chain")
	}

	return setupIPTables(&util.RealCmdRunner{}, recreateChain)
}

func InstallAll(cliCtx *cli.Context, runner util.CmdRunner, logger log.Logger, config *config.Config) error {
	logger.Info("Running executor install ignite")
	if err := installIgnite(cliCtx); err != nil {
		return err
	}

	logger.Info("Running executor install cni")
	if err := installCNIPlugins(cliCtx); err != nil {
		return err
	}

	logger.Info("Running executor install src-cli")
	if err := installSrc(cliCtx, logger, config); err != nil {
		return err
	}

	logger.Info("Running executor install iptables-rules")
	if err := setupIPTables(runner, false); err != nil {
		return err
	}

	logger.Info("Running executor install image executor-vm")
	if err := ensureExecutorVMImage(cliCtx.Context, runner, logger, config); err != nil {
		return err
	}

	logger.Info("Running executor install image sandbox")
	if err := ensureSandboxImage(cliCtx.Context, runner, logger, config); err != nil {
		return err
	}

	logger.Info("Running executor install image kernel")
	if err := ensureKernelImage(cliCtx.Context, runner, logger, config); err != nil {
		return err
	}

	return nil
}

func InstallImage(cliCtx *cli.Context, runner util.CmdRunner, logger log.Logger, config *config.Config) error {
	if !hostMightBeAbleToRunIgnite() {
		return ErrNoIgniteSupport
	}

	if !cliCtx.Args().Present() {
		return errors.New("no image specified")
	}
	if cliCtx.Args().Len() != 1 {
		return errors.New("too many arguments")
	}

	img := strings.ToLower(cliCtx.Args().First())
	switch img {
	case "executor-vm":
		return ensureExecutorVMImage(cliCtx.Context, runner, logger, config)
	case "sandbox":
		return ensureSandboxImage(cliCtx.Context, runner, logger, config)
	case "kernel":
		return ensureKernelImage(cliCtx.Context, runner, logger, config)
	default:
		return errors.Newf("invalid image provided %q, expected one of executor-vm, sandbox, kernel", img)
	}
}

func ensureExecutorVMImage(ctx context.Context, runner util.CmdRunner, logger log.Logger, c *config.Config) error {
	if err := util.ValidateIgniteInstalled(ctx, runner); err != nil {
		return err
	}

	// Make sure the image exists. When ignite imports these at runtime, there can
	// be a race condition and it is imported multiple times. Also, this would
	// happen for the first job, which is not desirable.
	logger.Info("Ensuring VM image is imported", log.String("image", c.FirecrackerImage))
	cmd := exec.CommandContext(ctx, "ignite", "image", "import", "--runtime", "docker", c.FirecrackerImage)
	// Forward output.
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		return errors.Wrapf(err, "importing ignite VM base image: %s", err)
	}
	return nil
}

func ensureKernelImage(ctx context.Context, runner util.CmdRunner, logger log.Logger, c *config.Config) error {
	if err := util.ValidateIgniteInstalled(ctx, runner); err != nil {
		return err
	}

	// Make sure the image exists. When ignite imports these at runtime, there can
	// be a race condition and it is imported multiple times. Also, this would
	// happen for the first job, which is not desirable.
	logger.Info("Ensuring kernel is imported", log.String("image", c.FirecrackerKernelImage))
	cmd := exec.CommandContext(ctx, "ignite", "kernel", "import", "--runtime", "docker", c.FirecrackerKernelImage)
	// Forward output.
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		return errors.Wrapf(err, "importing ignite kernel: %s", err)
	}
	return nil
}

func ensureSandboxImage(ctx context.Context, runner util.CmdRunner, logger log.Logger, c *config.Config) error {
	if err := util.ValidateIgniteInstalled(ctx, runner); err != nil {
		return err
	}

	// Make sure the image exists. When ignite imports these at runtime, there will
	// be a slowdown on the first job run.
	logger.Info("Ensuring sandbox image exists", log.String("image", c.FirecrackerSandboxImage))
	cmd := exec.CommandContext(ctx, "docker", "pull", c.FirecrackerSandboxImage)
	// Forward output.
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		return errors.Wrapf(err, "importing ignite isolation image: %s", err)
	}
	return nil
}

func setupIPTables(runner util.CmdRunner, recreateChain bool) error {
	found, err := util.ExistsPath(runner, "iptables")
	if err != nil {
		return errors.Wrap(err, "failed to look up iptables")
	}
	if !found {
		return errors.Newf("iptables not found, is it installed?")
	}

	// TODO: Use config.CNISubnetCIDR below instead of hard coded CIDRs.

	ipt, err := iptables.NewWithProtocol(iptables.ProtocolIPv4)
	if err != nil {
		return err
	}

	if recreateChain {
		if err = ipt.DeleteChain("filter", "CNI-ADMIN"); err != nil {
			return err
		}
	}

	// Ensure the chain exists.
	if ok, err := ipt.ChainExists("filter", "CNI-ADMIN"); err != nil {
		return err
	} else if !ok {
		if err = ipt.NewChain("filter", "CNI-ADMIN"); err != nil {
			return err
		}
	}

	// Explicitly allow DNS traffic (currently, the DNS server lives in the private
	// networks for GCP and AWS. Ideally we'd want to use an internet-only DNS server
	// to prevent leaking any network details).
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-p", "udp", "--dport", "53", "-j", "ACCEPT"); err != nil {
		return err
	}

	// Disallow any host-VM network traffic from the guests, except connections made
	// FROM the host (to ssh into the guest).
	if err = ipt.AppendUnique("filter", "INPUT", "-d", "10.61.0.0/16", "-m", "conntrack", "--ctstate", "RELATED,ESTABLISHED", "-j", "ACCEPT"); err != nil {
		return err
	}
	if err = ipt.AppendUnique("filter", "INPUT", "-s", "10.61.0.0/16", "-j", "DROP"); err != nil {
		return err
	}

	// Disallow any inter-VM traffic.
	// But allow to reach the gateway for internet access.
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-s", "10.61.0.1/32", "-d", "10.61.0.0/16", "-j", "ACCEPT"); err != nil {
		return err
	}
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-d", "10.61.0.0/16", "-m", "conntrack", "--ctstate", "RELATED,ESTABLISHED", "-j", "ACCEPT"); err != nil {
		return err
	}
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-s", "10.61.0.0/16", "-d", "10.61.0.0/16", "-j", "DROP"); err != nil {
		return err
	}

	// Disallow local networks access.
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-s", "10.61.0.0/16", "-d", "10.0.0.0/8", "-p", "tcp", "-j", "DROP"); err != nil {
		return err
	}
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-s", "10.61.0.0/16", "-d", "192.168.0.0/16", "-p", "tcp", "-j", "DROP"); err != nil {
		return err
	}
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-s", "10.61.0.0/16", "-d", "172.16.0.0/12", "-p", "tcp", "-j", "DROP"); err != nil {
		return err
	}
	// Disallow link-local traffic, too. This usually contains cloud provider
	// resources that we don't want to expose.
	if err = ipt.AppendUnique("filter", "CNI-ADMIN", "-s", "10.61.0.0/16", "-d", "169.254.0.0/16", "-j", "DROP"); err != nil {
		return err
	}

	return nil
}

func installIgnite(cliCtx *cli.Context) error {
	binDir := cliCtx.Path("bin-dir")
	if binDir == "" {
		binDir = "/usr/local/bin"
	}

	_, err := download.Executable(cliCtx.Context, fmt.Sprintf("https://github.com/sourcegraph/ignite/releases/download/%s/ignite-amd64", config.DefaultIgniteVersion), path.Join(binDir, "ignite"), true)
	if err != nil {
		return err
	}
	return nil
}

func installCNIPlugins(cliCtx *cli.Context) error {
	basePath := "/opt/cni/bin"
	if err := os.MkdirAll(basePath, os.ModePerm); err != nil {
		return err
	}
	for _, plugin := range config.RequiredCNIPlugins {
		if plugin == "isolation" {
			// Special case, handled below.
			continue
		}
		if err := download.ArchivedExecutable(cliCtx.Context, "https://github.com/containernetworking/plugins/releases/download/v0.9.1/cni-plugins-linux-amd64-v0.9.1.tgz", path.Join(basePath, plugin), plugin); err != nil {
			return err
		}

	}
	err := download.ArchivedExecutable(cliCtx.Context, "https://github.com/AkihiroSuda/cni-isolation/releases/download/v0.0.4/cni-isolation-amd64.tgz", path.Join(basePath, "isolation"), "isolation")
	if err != nil {
		return err
	}
	return nil
}

func installSrc(cliCtx *cli.Context, logger log.Logger, config *config.Config) error {
	binDir := cliCtx.Path("bin-dir")
	if binDir == "" {
		binDir = "/usr/local/bin"
	}

	copts := queueOptions(
		config,
		// We don't need telemetry here as we only use the client to talk to the Sourcegraph
		// instance to see what src-cli version it recommends. This saves a few exec calls
		// and confusing error messages.
		queue.TelemetryOptions{},
	)
	client, err := apiclient.NewBaseClient(logger, copts.BaseClientOptions)
	if err != nil {
		return err
	}
	srcVersion := srccli.MinimumVersion
	if copts.BaseClientOptions.EndpointOptions.URL != "" {
		srcVersion, err = util.LatestSrcCLIVersion(cliCtx.Context, client, copts.BaseClientOptions.EndpointOptions)
		if err != nil {
			logger.Warn("Failed to fetch latest src version, falling back to minimum version required by this executor", log.Error(err))
		}
	} else {
		logger.Warn("Sourcegraph instance endpoint not configured, using minimum src-cli version instead of recommended version")
	}

	return download.ArchivedExecutable(cliCtx.Context, fmt.Sprintf("https://github.com/sourcegraph/src-cli/releases/download/%s/src-cli_%s_%s_%s.tar.gz", srcVersion, srcVersion, runtime.GOOS, runtime.GOARCH), path.Join(binDir, "src"), "src")
}

var ErrNoIgniteSupport = errors.New("this host cannot run firecracker VMs, only linux hosts on amd64 processors are supported at the moment")

func hostMightBeAbleToRunIgnite() bool {
	return runtime.GOOS == "linux" && runtime.GOARCH == "amd64"
}
