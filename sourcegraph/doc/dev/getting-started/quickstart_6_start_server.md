# Quickstart step 6: Start the server

```bash
cd sourcegraph
./dev/start.sh
```

This will continuously compile your code and live reload your locally running instance of Sourcegraph.

Navigate your browser to https://sourcegraph.test:3443 to see if everything worked.

## Environment

Sourcegraph server is a collection of smaller binaries. The development server, [dev/start.sh](https://github.com/sourcegraph/sourcegraph/blob/main/dev/start.sh), initializes the environment and starts a process manager that runs all of the binaries. See the [Architecture doc](../background-information/architecture/index.md) for a full description of what each of these services does. The sections below describe the dependencies you need to run `dev/start.sh`.

<!-- omit in toc -->
## For Sourcegraph employees

You'll need to clone [`sourcegraph/dev-private`](https://github.com/sourcegraph/dev-private) (which has convenient preconfigured settings and external services on an enterprise account) alongside the `sourcegraph/sourcegraph` repository, for example:

```
/dir
 |-- dev-private
 +-- sourcegraph
```

After the initial setup you can run `enterprise/dev/start.sh` instead of `dev/start.sh`.

The environment variables `SITE_CONFIG_FILE`, `EXTSVC_CONFIG_FILE` and `GLOBAL_SETTINGS_FILE` are paths that are read at startup. The content of the files will overwrite the respective setting. `start.sh` will set these files to point into `dev-private`. To avoid overwriting configuration changes done in Sourcegraph, you can set the environment variable `DEV_NO_CONFIG=1`.

[< Previous](quickstart_5_configure_https_reverse_proxy.md) | [Next >](../how-to/troubleshooting_local_development.md)
