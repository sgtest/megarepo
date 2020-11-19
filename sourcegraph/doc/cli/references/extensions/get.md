# `src extensions get`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-extension-id` | Look up extension by extension ID. (e.g. "alice/myextension") |  |
| `-f` | Format for the output, using the syntax of Go package text/template. (e.g. "{{.ExtensionID}}: {{.Manifest.Title}} ({{.RemoteURL}})" or "{{.|json}}") | `{{.|json}}` |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |


## Usage

```
Usage of 'src extensions get':
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -extension-id string
    	Look up extension by extension ID. (e.g. "alice/myextension")
  -f string
    	Format for the output, using the syntax of Go package text/template. (e.g. "{{.ExtensionID}}: {{.Manifest.Title}} ({{.RemoteURL}})" or "{{.|json}}") (default "{{.|json}}")
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing

Examples:

  Get extension with extension ID "alice/myextension":

    	$ src extensions get alice/myextension
    	$ src extensions get -extension-id=alice/myextension



```
	