<!-- DO NOT EDIT: generated via: go generate ./enterprise/dev/ci -->

# Pipeline types reference

This is a reference outlining what CI pipelines we generate under different conditions.

To preview the pipeline for your branch, use `sg ci preview`.

For a higher-level overview, please refer to the [continuous integration docs](https://docs.sourcegraph.com/dev/background-information/ci).

## Run types

### Pull request

The default run type.

- Pipeline for `Go` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint

- Pipeline for `Client` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint
  - **Client checks**: Upload Storybook to Chromatic, Enterprise build, Build (client/jetbrains), Tests for VS Code extension, Unit, integration, and E2E tests for the Cody VS Code extension, ESLint (all), ESLint (web), Stylelint (all)
  - **Pipeline setup**: Trigger async

- Pipeline for `GraphQL` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Client checks**: Upload Storybook to Chromatic, Enterprise build, Build (client/jetbrains), Tests for VS Code extension, Unit, integration, and E2E tests for the Cody VS Code extension, ESLint (all), ESLint (web), Stylelint (all)

- Pipeline for `DatabaseSchema` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `Docs` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint

- Pipeline for `Dockerfiles` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint

- Pipeline for `ExecutorVMImage` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `ExecutorDockerRegistryMirror` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `CIScripts` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `Terraform` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `SVG` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint

- Pipeline for `Shell` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint

- Pipeline for `DockerImages` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `WolfiPackages` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `WolfiBaseImages` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests

- Pipeline for `Protobuf` changes:
  - **Metadata**: Pipeline metadata
  - Ensure buildfiles are up to date
  - Tests
  - BackCompat Tests
  - **Linters and static analysis**: Run sg lint

### Wolfi Exp Branch

The run type for branches matching `wolfi/`.
You can create a build of this run type for your changes using:

```sh
sg ci build wolfi
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Wolfi image builds**: Build Wolfi-based batcheshelper, Build Wolfi-based blobstore, Build Wolfi-based bundled-executor, Build Wolfi-based cadvisor, Build Wolfi-based embeddings, Build Wolfi-based executor, Build Wolfi-based executor-kubernetes, Build Wolfi-based frontend, Build Wolfi-based github-proxy, Build Wolfi-based gitserver, Build Wolfi-based indexed-searcher, Build Wolfi-based jaeger-agent, Build Wolfi-based jaeger-all-in-one, Build Wolfi-based cody-gateway, Build Wolfi-based loadtest, Build Wolfi-based migrator, Build Wolfi-based node-exporter, Build Wolfi-based opentelemetry-collector, Build Wolfi-based postgres_exporter, Build Wolfi-based precise-code-intel-worker, Build Wolfi-based prometheus, Build Wolfi-based prometheus-gcp, Build Wolfi-based redis-cache, Build Wolfi-based redis-store, Build Wolfi-based redis_exporter, Build Wolfi-based repo-updater, Build Wolfi-based search-indexer, Build Wolfi-based searcher, Build Wolfi-based server, Build Wolfi-based sg, Build Wolfi-based symbols, Build Wolfi-based syntax-highlighter, Build Wolfi-based worker

### Release branch nightly healthcheck build

The run type for environment including `{"RELEASE_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- Trigger 5.0 release branch healthcheck build
- Trigger 4.5 release branch healthcheck build

### Browser extension nightly release build

The run type for environment including `{"BEXT_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- ESLint (web)
- Stylelint (all)
- Test (client/browser)
- Puppeteer tests for chrome extension
- Test (all)
- E2E for chrome extension

### VS Code extension nightly release build

The run type for environment including `{"VSCE_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- ESLint (web)
- Stylelint (all)
- Tests for VS Code extension

### Cody VS Code extension nightly release build

The run type for environment including `{"CODY_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- ESLint (web)
- Stylelint (all)
- Unit, integration, and E2E tests for the Cody VS Code extension
- Cody release

### App release build

The run type for branches matching `app/release` (exact match).

Base pipeline (more steps might be included based on branch changes):

- App release

### App insiders build

The run type for branches matching `app/insiders` (exact match).

Base pipeline (more steps might be included based on branch changes):

- App release

### Tagged release

The run type for tags starting with `v`.

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build syntax-highlighter, Build symbols, Build Docker images, Build Docker images, Build Docker images, Build executor image, Build executor binary, Build docker registry mirror image
- **Image security scans**: Scan executor, Scan alpine-3.14, Scan postgres-12-alpine, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan migrator, Scan node-exporter, Scan opentelemetry-collector, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan syntax-highlighter, Scan worker, Scan symbols, Scan batcheshelper, Scan blobstore2, Scan bundled-executor, Scan dind, Scan embeddings, Scan executor-kubernetes, Scan executor-vm, Scan jaeger-agent, Scan jaeger-all-in-one, Scan cody-gateway, Scan sg, Scan cody-slack
- Ensure buildfiles are up to date
- Tests
- BackCompat Tests
- **Linters and static analysis**: Run sg lint
- **Client checks**: Upload Storybook to Chromatic, Enterprise build, Build (client/jetbrains), Tests for VS Code extension, Unit, integration, and E2E tests for the Cody VS Code extension, ESLint (all), ESLint (web), Stylelint (all)
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph Upgrade
- **Publish images**: server, executor, alpine-3.14, postgres-12-alpine, blobstore, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, migrator, node-exporter, opentelemetry-collector, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, syntax-highlighter, worker, symbols, batcheshelper, blobstore2, bundled-executor, dind, embeddings, executor-kubernetes, executor-vm, jaeger-agent, jaeger-all-in-one, cody-gateway, sg, cody-slack, Publish executor image, Publish executor binary, Publish docker registry mirror image, Push OCI/Wolfi

### Release branch

The run type for branches matching `^[0-9]+\.[0-9]+$` (regexp match).

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build syntax-highlighter, Build symbols, Build Docker images, Build Docker images, Build Docker images, Build executor image, Build executor binary, Build docker registry mirror image
- **Image security scans**: Scan executor, Scan alpine-3.14, Scan postgres-12-alpine, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan migrator, Scan node-exporter, Scan opentelemetry-collector, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan syntax-highlighter, Scan worker, Scan symbols, Scan batcheshelper, Scan blobstore2, Scan bundled-executor, Scan dind, Scan embeddings, Scan executor-kubernetes, Scan executor-vm, Scan jaeger-agent, Scan jaeger-all-in-one, Scan cody-gateway, Scan sg, Scan cody-slack
- Ensure buildfiles are up to date
- Tests
- BackCompat Tests
- **Linters and static analysis**: Run sg lint
- **Client checks**: Upload Storybook to Chromatic, Enterprise build, Build (client/jetbrains), Tests for VS Code extension, Unit, integration, and E2E tests for the Cody VS Code extension, ESLint (all), ESLint (web), Stylelint (all)
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph Upgrade
- **Publish images**: server, executor, alpine-3.14, postgres-12-alpine, blobstore, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, migrator, node-exporter, opentelemetry-collector, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, syntax-highlighter, worker, symbols, batcheshelper, blobstore2, bundled-executor, dind, embeddings, executor-kubernetes, executor-vm, jaeger-agent, jaeger-all-in-one, cody-gateway, sg, cody-slack, Push OCI/Wolfi

### Browser extension release build

The run type for branches matching `bext/release` (exact match).

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- ESLint (web)
- Stylelint (all)
- Test (client/browser)
- Puppeteer tests for chrome extension
- Test (all)
- E2E for chrome extension
- Extension release
- Extension release
- npm Release

### VS Code extension release build

The run type for branches matching `vsce/release` (exact match).

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- ESLint (web)
- Stylelint (all)
- Tests for VS Code extension
- Extension release

### Cody VS Code extension release build

The run type for branches matching `cody/release` (exact match).

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- ESLint (web)
- Stylelint (all)
- Unit, integration, and E2E tests for the Cody VS Code extension
- Cody release

### Main branch

The run type for branches matching `main` (exact match).

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build syntax-highlighter, Build symbols, Build Docker images, Build Docker images, Build Docker images, Build executor image, Build executor binary
- **Image security scans**: Scan executor, Scan alpine-3.14, Scan postgres-12-alpine, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan migrator, Scan node-exporter, Scan opentelemetry-collector, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan syntax-highlighter, Scan worker, Scan symbols, Scan batcheshelper, Scan blobstore2, Scan bundled-executor, Scan dind, Scan embeddings, Scan executor-kubernetes, Scan executor-vm, Scan jaeger-agent, Scan jaeger-all-in-one, Scan cody-gateway, Scan sg, Scan cody-slack
- Ensure buildfiles are up to date
- Tests
- BackCompat Tests
- **Linters and static analysis**: Run sg lint
- **Client checks**: Upload Storybook to Chromatic, Enterprise build, Build (client/jetbrains), Tests for VS Code extension, Unit, integration, and E2E tests for the Cody VS Code extension, ESLint (all), ESLint (web), Stylelint (all)
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph Upgrade
- **Publish images**: server, executor, alpine-3.14, postgres-12-alpine, blobstore, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, migrator, node-exporter, opentelemetry-collector, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, syntax-highlighter, worker, symbols, batcheshelper, blobstore2, bundled-executor, dind, embeddings, executor-kubernetes, executor-vm, jaeger-agent, jaeger-all-in-one, cody-gateway, sg, cody-slack, Publish executor image, Publish executor binary, Push OCI/Wolfi

### Main dry run

The run type for branches matching `main-dry-run/`.
You can create a build of this run type for your changes using:

```sh
sg ci build main-dry-run
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build syntax-highlighter, Build symbols, Build Docker images, Build Docker images, Build Docker images, Build executor image, Build executor binary
- **Image security scans**: Scan executor, Scan alpine-3.14, Scan postgres-12-alpine, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan migrator, Scan node-exporter, Scan opentelemetry-collector, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan syntax-highlighter, Scan worker, Scan symbols, Scan batcheshelper, Scan blobstore2, Scan bundled-executor, Scan dind, Scan embeddings, Scan executor-kubernetes, Scan executor-vm, Scan jaeger-agent, Scan jaeger-all-in-one, Scan cody-gateway, Scan sg, Scan cody-slack
- Ensure buildfiles are up to date
- Tests
- BackCompat Tests
- **Linters and static analysis**: Run sg lint
- **Client checks**: Upload Storybook to Chromatic, Enterprise build, Build (client/jetbrains), Tests for VS Code extension, Unit, integration, and E2E tests for the Cody VS Code extension, ESLint (all), ESLint (web), Stylelint (all)
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph Upgrade
- **Publish images**: server, executor, alpine-3.14, postgres-12-alpine, blobstore, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, migrator, node-exporter, opentelemetry-collector, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, syntax-highlighter, worker, symbols, batcheshelper, blobstore2, bundled-executor, dind, embeddings, executor-kubernetes, executor-vm, jaeger-agent, jaeger-all-in-one, cody-gateway, sg, cody-slack, Push OCI/Wolfi

### Patch image

The run type for branches matching `docker-images-patch/`, requires a branch argument in the second branch path segment.
You can create a build of this run type for your changes using:

```sh
sg ci build docker-images-patch
```

### Patch image without testing

The run type for branches matching `docker-images-patch-notest/`, requires a branch argument in the second branch path segment.
You can create a build of this run type for your changes using:

```sh
sg ci build docker-images-patch-notest
```

### Build all candidates without testing

The run type for branches matching `docker-images-candidates-notest/`.
You can create a build of this run type for your changes using:

```sh
sg ci build docker-images-candidates-notest
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Image builds**: Build syntax-highlighter, Build symbols, Build Docker images, Build Docker images, Build Docker images
- **Publish images**: Publish images

### Build executor without testing

The run type for branches matching `executor-patch-notest/`.
You can create a build of this run type for your changes using:

```sh
sg ci build executor-patch-notest
```

Base pipeline (more steps might be included based on branch changes):

- Build executor-vm
- Scan executor-vm
- Build executor image
- Build docker registry mirror image
- Build executor binary
- executor-vm
- Publish executor image
- Publish docker registry mirror image
- Publish executor binary

### Backend integration tests

The run type for branches matching `backend-integration/`.
You can create a build of this run type for your changes using:

```sh
sg ci build backend-integration
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- Build server
- Backend integration tests (gRPC)
- Backend integration tests
