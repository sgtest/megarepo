# `src extensions list`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-f` | Format for the output, using the syntax of Go package text/template. (e.g. "{{.ExtensionID}}: {{.Manifest.Description}} ({{.RemoteURL}})" or "{{.|json}}") | `{{.ExtensionID}}` |
| `-first` | Returns the first n extensions from the list. (use -1 for unlimited) | `1000` |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-query` | Returns extensions whose extension IDs match the query. (e.g. "myextension") |  |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |


## Usage

```
Usage of 'src extensions list':
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -f string
    	Format for the output, using the syntax of Go package text/template. (e.g. "{{.ExtensionID}}: {{.Manifest.Description}} ({{.RemoteURL}})" or "{{.|json}}") (default "{{.ExtensionID}}")
  -first int
    	Returns the first n extensions from the list. (use -1 for unlimited) (default 1000)
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -query string
    	Returns extensions whose extension IDs match the query. (e.g. "myextension")
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing

Examples:

  List extensions:

    	$ src extensions list

  List extensions whose names match the query:

    	$ src extensions list -query='myquery'

  List *all* extensions (may be slow!):

    	$ src extensions list -first='-1'



```
	