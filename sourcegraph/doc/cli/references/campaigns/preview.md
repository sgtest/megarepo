# `src campaigns preview`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-allow-unsupported` | Allow unsupported code hosts. | `false` |
| `-apply` | Ignored. | `false` |
| `-cache` | Directory for caching results and repository archives. | `~/.cache/sourcegraph/campaigns` |
| `-clean-archives` | If true, deletes downloaded repository archives after executing campaign steps. | `true` |
| `-clear-cache` | If true, clears the execution cache and executes all steps anew. | `false` |
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-f` | The campaign spec file to read. |  |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-j` | The maximum number of parallel jobs. (Default: GOMAXPROCS.) | `0` |
| `-keep-logs` | Retain logs after executing steps. | `false` |
| `-n` | Alias for -namespace. |  |
| `-namespace` | The user or organization namespace to place the campaign within. Default is the currently authenticated user. |  |
| `-timeout` | The maximum duration a single set of campaign steps can take. | `1h0m0s` |
| `-tmp` | Directory for storing temporary data, such as log files. Default is /tmp. Can also be set with environment variable SRC_CAMPAIGNS_TMP_DIR; if both are set, this flag will be used and not the environment variable. | `/tmp` |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |


## Usage

```
Usage of 'src campaigns preview':
  -allow-unsupported
    	Allow unsupported code hosts.
  -apply
    	Ignored.
  -cache string
    	Directory for caching results and repository archives. (default "~/.cache/sourcegraph/campaigns")
  -clean-archives
    	If true, deletes downloaded repository archives after executing campaign steps. (default true)
  -clear-cache
    	If true, clears the execution cache and executes all steps anew.
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -f string
    	The campaign spec file to read.
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -j int
    	The maximum number of parallel jobs. (Default: GOMAXPROCS.)
  -keep-logs
    	Retain logs after executing steps.
  -n string
    	Alias for -namespace.
  -namespace string
    	The user or organization namespace to place the campaign within. Default is the currently authenticated user.
  -timeout duration
    	The maximum duration a single set of campaign steps can take. (default 1h0m0s)
  -tmp string
    	Directory for storing temporary data, such as log files. Default is /tmp. Can also be set with environment variable SRC_CAMPAIGNS_TMP_DIR; if both are set, this flag will be used and not the environment variable. (default "/tmp")
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing

'src campaigns preview' executes the steps in a campaign spec and uploads it to
a Sourcegraph instance, ready to be previewed and applied.

Usage:

    src campaigns preview -f FILE [command options]

Examples:

    $ src campaigns preview -f campaign.spec.yaml



```
	