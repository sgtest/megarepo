# Code graph context

Code graph context is Cody's ability to respond to queries using contextual information of a codebase. We recommend configuring code graph context for the best Cody experience.

## Relevant code files

Cody reads relevant code files to increase the accuracy and quality of the response and make it match your own codebase's conventions. There are 2 ways Cody can find relevant code files: embeddings (preferred) and local keyword-based search.

### Embeddings

Embeddings are a semantic representation of text that allow us to create a search index over an entire codebase. The process of creating embeddings involves us splitting the entire codebase into searchable chunks and sending them to the external service specified in the site config for embedding. The final embedding index is stored in a managed object storage service.

Embeddings for relevant code files must be enabled for each repository that you'd like Cody to have context on.

### Configuring embeddings

> NOTE: Enterprise Cloud customers should reach out to their Sourcegraph representative to enable embeddings. 
> See [Enabling Cody Enterprise](./enabling_cody_enterprise.md#cody-on-sourcegraph-cloud)

1. Go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance
1. Add the following to configure OpenAI embeddings:
    ```json
    {
      // [...]
      "embeddings": {
        "enabled": true,
        "url": "https://api.openai.com/v1/embeddings",
        "accessToken": "<token>",
        "model": "text-embedding-ada-002",
        "dimensions": 1536,
        "excludedFilePathPatterns": []
      }
    }
    ```
1. Navigate to **Site admin > Cody** (`/site-admin/cody`) and schedule repositories for embedding.

> NOTE: By enabling Cody, you agree to the [Cody Notice and Usage Policy](https://about.sourcegraph.com/terms/cody-notice). 

### Excluding files from embeddings

The `excludedFilePathPatterns` is a setting in the Sourcegraph embeddings configuration that allows you to exclude certain file paths from being used in generating embeddings. By specifying glob patterns that match file paths, you can exclude files that have low information value, such as test fixtures, mocks, auto-generated files, and other files that are not relevant to the codebase.

To use `excludedFilePathPatterns`, add it to your embeddings site config with a list of glob patterns. For example, to exclude all SVG files, you would add the following setting to your configuration file:

```json
{
  // [...]
  "embeddings": {
    // [...]
    "excludedFilePathPatterns": [
      "*.svg"
    ]
  }
}
```

> NOTE: The `excludedFilePathPatterns` setting is only available in Sourcegraph version `5.0.1` and later.

### Storing embedding indexes

To target a managed object storage service, you will need to set a handful of environment variables for configuration and authentication to the target service. **If you are running a sourcegraph/server deployment, set the environment variables on the server container. Otherwise, if running via Docker-compose or Kubernetes, set the environment variables on the `frontend`, `embeddings`, and `worker` containers.**

#### Using S3

To target an S3 bucket you've already provisioned, set the following environment variables. Authentication can be done through [an access and secret key pair](https://docs.aws.amazon.com/general/latest/gr/aws-sec-cred-types.html#access-keys-and-secret-access-keys) (and optional session token), or via the EC2 metadata API.

**_Warning:_** Remember never to commit aws access keys in git. Consider using a secret handling service offered by your cloud provider. 

- `EMBEDDINGS_UPLOAD_BACKEND=S3`
- `EMBEDDINGS_UPLOAD_BUCKET=<my bucket name>`
- `EMBEDDINGS_UPLOAD_AWS_ENDPOINT=https://s3.us-east-1.amazonaws.com`
- `EMBEDDINGS_UPLOAD_AWS_ACCESS_KEY_ID=<your access key>`
- `EMBEDDINGS_UPLOAD_AWS_SECRET_ACCESS_KEY=<your secret key>`
- `EMBEDDINGS_UPLOAD_AWS_SESSION_TOKEN=<your session token>` (optional)
- `EMBEDDINGS_UPLOAD_AWS_USE_EC2_ROLE_CREDENTIALS=true` (optional; set to use EC2 metadata API over static credentials)
- `EMBEDDINGS_UPLOAD_AWS_REGION=us-east-1` (default)

**_Note:_** If a non-default region is supplied, ensure that the subdomain of the endpoint URL (_the `AWS_ENDPOINT` value_) matches the target region.

> NOTE: You don't need to set the `EMBEDDINGS_UPLOAD_AWS_ACCESS_KEY_ID` environment variable when using `EMBEDDINGS_UPLOAD_AWS_USE_EC2_ROLE_CREDENTIALS=true` because role credentials will be automatically resolved. 


### Using GCS

To target a GCS bucket you've already provisioned, set the following environment variables. Authentication is done through a service account key, supplied as either a path to a volume-mounted file, or the contents read in as an environment variable payload.

- `EMBEDDINGS_UPLOAD_BACKEND=GCS`
- `EMBEDDINGS_UPLOAD_BUCKET=<my bucket name>`
- `EMBEDDINGS_UPLOAD_GCP_PROJECT_ID=<my project id>`
- `EMBEDDINGS_UPLOAD_GOOGLE_APPLICATION_CREDENTIALS_FILE=</path/to/file>`
- `EMBEDDINGS_UPLOAD_GOOGLE_APPLICATION_CREDENTIALS_FILE_CONTENT=<{"my": "content"}>`

#### Provisioning buckets

If you would like to allow your Sourcegraph instance to control the creation and lifecycle configuration management of the target buckets, set the following environment variables:

- `EMBEDDINGS_UPLOAD_MANAGE_BUCKET=true`

### Environment variables for the `embeddings` service

- `EMBEDDINGS_CACHE_SIZE`: The maximum size of the in-memory cache (in bytes) that holds the embeddings for commonly-searched repos. If embeddings for a repo are larger than this size, the repo will not be held in the cache and must be re-fetched for each embeddings search. Defaults to `6442450944` (6 GiB).

### Incremental embeddings

Incremental embeddings allow you to update the embeddings for a repository without having to re-embed the entire
repository. With incremental embeddings, outdated embeddings of deleted and modified files are removed and new
embeddings of the modified and added files are added to the repository's embeddings. This speeds up updates, reduces the
data sent to the embedding provider and saves costs.

Incremental embeddings are enabled by default. You can disable incremental embeddings by setting
the `incremental` property in the embeddings configuration to `false`.

```json
{
  // [...]
  "embeddings": {
    // [...]
    "incremental": false
  }
}
```

### Adjust the minimum time interval between automatically scheduled embeddings

If you configure a repository for automated embeddings, the repository will be scheduled for embedding with every new
commit. By default, there is a 24-hour time interval that must pass between two embeddings. For example, if a repository
is scheduled for embedding at 10:00 AM and a new commit happens at 11:00 AM, the next embedding will be scheduled
earliest for 10:00 AM the next day.

You can configure the minimum time interval by setting the minimumInterval property in the embeddings configuration.
Supported time units are h (hours), m ( minutes), and s (seconds).

```json
{
  // [...]
  "embeddings": {
    // [...]
    "minimumInterval": "24h"
  }
}
```
