# `src orgs create`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-display-name` | The new organization's display name. |  |
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-name` | The new organization's name. (required) |  |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |


## Usage

```
Usage of 'src orgs create':
  -display-name string
    	The new organization's display name.
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -name string
    	The new organization's name. (required)
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing

Examples:

  Create an organization:

    	$ src orgs create -name=abc-org -display-name='ABC Organization'



```
	