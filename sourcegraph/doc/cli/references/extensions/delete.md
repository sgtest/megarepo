# `src extensions delete`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-id` | The ID (GraphQL API ID, not extension ID) of the extension to delete. |  |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |


## Usage

```
Usage of 'src extensions delete':
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -id string
    	The ID (GraphQL API ID, not extension ID) of the extension to delete.
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing

Examples:

  Delete the extension by ID (GraphQL API ID, not extension ID):

    	$ src extensions delete -id=UmVnaXN0cnlFeHRlbnNpb246...

  Delete the extension with extension ID "alice/myextension":

    	$ src extensions delete -id=$(src extensions get -f '{{.ID}}' -extension-id=alice/myextension)



```
	