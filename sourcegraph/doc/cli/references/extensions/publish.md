# `src extensions publish`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-dump-requests` | Log GraphQL requests and responses to stdout | `false` |
| `-extension-id` | Override the extension ID in the manifest. (default: read from -manifest file) |  |
| `-force` | Force publish the extension, even if there are validation problems or other warnings. | `false` |
| `-get-curl` | Print the curl command for executing this query and exit (WARNING: includes printing your access token!) | `false` |
| `-manifest` | The extension manifest file. | `package.json` |
| `-trace` | Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing | `false` |
| `-url` | Override the URL for the bundle. (example: set to http://localhost:1234/myext.js for local dev with parcel) |  |


## Usage

```
Usage of 'src extensions publish':
  -dump-requests
    	Log GraphQL requests and responses to stdout
  -extension-id string
    	Override the extension ID in the manifest. (default: read from -manifest file)
  -force
    	Force publish the extension, even if there are validation problems or other warnings.
  -get-curl
    	Print the curl command for executing this query and exit (WARNING: includes printing your access token!)
  -manifest string
    	The extension manifest file. (default "package.json")
  -trace
    	Log the trace ID for requests. See https://docs.sourcegraph.com/admin/observability/tracing
  -url string
    	Override the URL for the bundle. (example: set to http://localhost:1234/myext.js for local dev with parcel)

Publish an extension to Sourcegraph, creating it (if necessary).

Examples:

  Publish the "alice/myextension" extension described by package.json in the current directory:

    	$ cat package.json
        {
          "name":      "myextension",
          "publisher": "alice",
          "title":     "My Extension",
          "main":      "dist/myext.js",
          "scripts":   {"sourcegraph:prepublish": "parcel build --out-file dist/myext.js src/myext.ts"}
        }
    	$ src extensions publish

Notes:

  Source maps are supported (for easier debugging of extensions). If the main JavaScript bundle is "dist/myext.js",
  it looks for a source map in "dist/myext.map".



```
	