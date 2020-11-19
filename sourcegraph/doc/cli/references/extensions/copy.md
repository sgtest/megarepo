# `src extensions copy`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-current-user` | The current user |  |
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-extension-id` | The <extID> in https://sourcegraph.com/extensions/<extID> (e.g. sourcegraph/java) |  |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |


## Usage

```
Usage of 'src extensions copy':
  -current-user string
    	The current user
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -extension-id string
    	The <extID> in https://sourcegraph.com/extensions/<extID> (e.g. sourcegraph/java)
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing

Copy an extension from Sourcegraph.com to your private registry.


```
	