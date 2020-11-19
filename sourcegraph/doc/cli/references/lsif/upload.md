# `src lsif upload`


## Flags

| Name | Description | Default Value |
|------|-------------|---------------|
| `-commit` | The 40-character hash of the commit. Defaults to the currently checked-out commit. |  |
| `-file` | The path to the LSIF dump file. | `./dump.lsif` |
| `-github-token` | A GitHub access token with 'public_repo' scope that Sourcegraph uses to verify you have access to the repository. |  |
| `-ignore-upload-failure` | Exit with status code zero on upload failure. | `false` |
| `-indexer` | The name of the indexer that generated the dump. This will override the 'toolInfo.name' field in the metadata vertex of the LSIF dump file. This must be supplied if the indexer does not set this field (in which case the upload will fail with an explicit message). |  |
| `-json` | Output relevant state in JSON on success. | `false` |
| `-max-payload-size` | The maximum upload size (in megabytes). Indexes exceeding this limit will be uploaded over multiple HTTP requests. | `100` |
| `-no-progress` | Do not display a progress bar. | `false` |
| `-open` | Open the LSIF upload page in your browser. | `false` |
| `-repo` | The name of the repository (e.g. github.com/gorilla/mux). By default, derived from the origin remote. |  |
| `-root` | The path in the repository that matches the LSIF projectRoot (e.g. cmd/project1). Defaults to the directory where the dump file is located. |  |
| `-upload-route` | The path of the upload route. | `/.api/lsif/upload` |


## Usage

```
Usage of 'src lsif upload':
  -commit string
    	The 40-character hash of the commit. Defaults to the currently checked-out commit.
  -file string
    	The path to the LSIF dump file. (default "./dump.lsif")
  -github-token string
    	A GitHub access token with 'public_repo' scope that Sourcegraph uses to verify you have access to the repository.
  -ignore-upload-failure
    	Exit with status code zero on upload failure.
  -indexer string
    	The name of the indexer that generated the dump. This will override the 'toolInfo.name' field in the metadata vertex of the LSIF dump file. This must be supplied if the indexer does not set this field (in which case the upload will fail with an explicit message).
  -json
    	Output relevant state in JSON on success.
  -max-payload-size int
    	The maximum upload size (in megabytes). Indexes exceeding this limit will be uploaded over multiple HTTP requests. (default 100)
  -no-progress
    	Do not display a progress bar.
  -open
    	Open the LSIF upload page in your browser.
  -repo string
    	The name of the repository (e.g. github.com/gorilla/mux). By default, derived from the origin remote.
  -root string
    	The path in the repository that matches the LSIF projectRoot (e.g. cmd/project1). Defaults to the directory where the dump file is located.
  -upload-route string
    	The path of the upload route. (default "/.api/lsif/upload")

Examples:

  Upload an LSIF dump with explicit repo, commit, and upload files:

    	$ src lsif upload -repo=FOO -commit=BAR -file=dump.lsif

  Upload an LSIF dump for a subproject:

    	$ src lsif upload -root=cmd/

  Upload an LSIF dump when lsifEnforceAuth is enabled:

    	$ src lsif upload -github-token=BAZ

  Upload an LSIF dump when the LSIF indexer does not not declare a tool name.

    	$ src lsif upload -indexer=lsif-elixir


```
	