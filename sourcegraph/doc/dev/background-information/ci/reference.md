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
  - **Linters and static analysis**: Run sg lint
  - **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
  - Upload build trace

- Pipeline for `Client` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: Run sg lint
  - **Client checks**: Puppeteer tests prep, Puppeteer tests for chrome extension, Puppeteer tests chunk #1, Puppeteer tests chunk #2, Puppeteer tests chunk #3, Puppeteer tests chunk #4, Puppeteer tests chunk #5, Puppeteer tests chunk #6, Puppeteer tests chunk #7, Puppeteer tests chunk #8, Puppeteer tests chunk #9, Puppeteer tests chunk #10, Puppeteer tests chunk #11, Puppeteer tests chunk #12, Puppeteer tests chunk #13, Puppeteer tests chunk #14, Puppeteer tests chunk #15, Upload Storybook to Chromatic, Test (all), Build, Enterprise build, Test (client/web), Test (client/browser), Test (client/jetbrains), Build TS, ESLint (all), Stylelint (all)
  - **Pipeline setup**: Trigger async
  - Client PR preview
  - Upload build trace

- Pipeline for `GraphQL` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: GraphQL lint
  - **Client checks**: Puppeteer tests prep, Puppeteer tests for chrome extension, Puppeteer tests chunk #1, Puppeteer tests chunk #2, Puppeteer tests chunk #3, Puppeteer tests chunk #4, Puppeteer tests chunk #5, Puppeteer tests chunk #6, Puppeteer tests chunk #7, Puppeteer tests chunk #8, Puppeteer tests chunk #9, Puppeteer tests chunk #10, Puppeteer tests chunk #11, Puppeteer tests chunk #12, Puppeteer tests chunk #13, Puppeteer tests chunk #14, Puppeteer tests chunk #15, Upload Storybook to Chromatic, Test (all), Build, Enterprise build, Test (client/web), Test (client/browser), Test (client/jetbrains), Build TS, ESLint (all), Stylelint (all)
  - **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
  - Upload build trace

- Pipeline for `DatabaseSchema` changes:
  - **Metadata**: Pipeline metadata
  - **DB backcompat tests**: Backcompat test (all), Backcompat test (all (gRPC)), Backcompat test (enterprise/internal/insights), Backcompat test (enterprise/internal/insights (gRPC)), Backcompat test (internal/repos), Backcompat test (internal/repos (gRPC)), Backcompat test (enterprise/internal/batches), Backcompat test (enterprise/internal/batches (gRPC)), Backcompat test (cmd/frontend), Backcompat test (cmd/frontend (gRPC)), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Backcompat test (dev/sg), Backcompat test (dev/sg (gRPC)), Backcompat test (internal/database), Backcompat test (enterprise/internal/database)
  - Upload build trace

- Pipeline for `Docs` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: Run sg lint
  - Upload build trace

- Pipeline for `Dockerfiles` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: Run sg lint
  - Upload build trace

- Pipeline for `ExecutorVMImage` changes:
  - **Metadata**: Pipeline metadata
  - Upload build trace

- Pipeline for `ExecutorDockerRegistryMirror` changes:
  - **Metadata**: Pipeline metadata
  - Upload build trace

- Pipeline for `CIScripts` changes:
  - **Metadata**: Pipeline metadata
  - **CI script tests**: test-trace-command.sh
  - Upload build trace

- Pipeline for `Terraform` changes:
  - **Metadata**: Pipeline metadata
  - Upload build trace

- Pipeline for `SVG` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: Run sg lint
  - Upload build trace

- Pipeline for `Shell` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: Run sg lint
  - Upload build trace

- Pipeline for `DockerImages` changes:
  - **Metadata**: Pipeline metadata
  - **Test builds**: Build alpine-3.14, Build cadvisor, Build codeinsights-db, Build codeintel-db, Build frontend, Build github-proxy, Build gitserver, Build grafana, Build indexed-searcher, Build jaeger-agent, Build jaeger-all-in-one, Build blobstore, Build blobstore2, Build node-exporter, Build postgres-12-alpine, Build postgres_exporter, Build precise-code-intel-worker, Build prometheus, Build prometheus-gcp, Build redis-cache, Build redis-store, Build redis_exporter, Build repo-updater, Build search-indexer, Build searcher, Build symbols, Build syntax-highlighter, Build worker, Build migrator, Build executor, Build executor-vm, Build batcheshelper, Build opentelemetry-collector, Build server, Build sg
  - **Scan test builds**: Scan alpine-3.14, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan jaeger-agent, Scan jaeger-all-in-one, Scan blobstore2, Scan node-exporter, Scan postgres-12-alpine, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan symbols, Scan syntax-highlighter, Scan worker, Scan migrator, Scan executor, Scan executor-vm, Scan batcheshelper, Scan opentelemetry-collector, Scan sg
  - Upload build trace

- Pipeline for `WolfiPackages` changes:
  - **Metadata**: Pipeline metadata
  - Upload build trace

- Pipeline for `WolfiBaseImages` changes:
  - **Metadata**: Pipeline metadata
  - Upload build trace

- Pipeline for `Protobuf` changes:
  - **Metadata**: Pipeline metadata
  - **Linters and static analysis**: Run sg lint
  - Upload build trace

### Bazel Exp Branch

The run type for branches matching `bzl/`.
You can create a build of this run type for your changes using:

```sh
sg ci build bzl
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- Build ...
- Tests
- Upload build trace

### Wolfi Exp Branch

The run type for branches matching `wolfi/`.
You can create a build of this run type for your changes using:

```sh
sg ci build wolfi
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- Upload build trace

### Release branch nightly healthcheck build

The run type for environment including `{"RELEASE_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- Trigger 4.5 release branch healthcheck build
- Trigger 4.4 release branch healthcheck build
- Upload build trace

### Browser extension nightly release build

The run type for environment including `{"BEXT_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- Stylelint (all)
- Test (client/browser)
- Puppeteer tests for chrome extension
- Test (all)
- E2E for chrome extension
- Upload build trace

### VS Code extension nightly release build

The run type for environment including `{"VSCE_NIGHTLY":"true"}`.

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- Stylelint (all)
- Upload build trace

### App snapshot release

The run type for branches matching `app/release-snapshot` (exact match).

Base pipeline (more steps might be included based on branch changes):

- App release
- Upload build trace

### Tagged release

The run type for tags starting with `v`.

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build alpine-3.14, Build cadvisor, Build codeinsights-db, Build codeintel-db, Build frontend, Build github-proxy, Build gitserver, Build grafana, Build indexed-searcher, Build jaeger-agent, Build jaeger-all-in-one, Build blobstore, Build blobstore2, Build node-exporter, Build postgres-12-alpine, Build postgres_exporter, Build precise-code-intel-worker, Build prometheus, Build prometheus-gcp, Build redis-cache, Build redis-store, Build redis_exporter, Build repo-updater, Build search-indexer, Build searcher, Build symbols, Build syntax-highlighter, Build worker, Build migrator, Build executor, Build executor-vm, Build batcheshelper, Build opentelemetry-collector, Build server, Build sg, Build executor image, Build executor binary, Build docker registry mirror image
- **Image security scans**: Scan alpine-3.14, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan jaeger-agent, Scan jaeger-all-in-one, Scan blobstore2, Scan node-exporter, Scan postgres-12-alpine, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan symbols, Scan syntax-highlighter, Scan worker, Scan migrator, Scan executor, Scan executor-vm, Scan batcheshelper, Scan opentelemetry-collector, Scan sg
- **Linters and static analysis**: GraphQL lint, Run sg lint
- **Client checks**: Puppeteer tests prep, Puppeteer tests for chrome extension, Puppeteer tests chunk #1, Puppeteer tests chunk #2, Puppeteer tests chunk #3, Puppeteer tests chunk #4, Puppeteer tests chunk #5, Puppeteer tests chunk #6, Puppeteer tests chunk #7, Puppeteer tests chunk #8, Puppeteer tests chunk #9, Puppeteer tests chunk #10, Puppeteer tests chunk #11, Puppeteer tests chunk #12, Puppeteer tests chunk #13, Puppeteer tests chunk #14, Puppeteer tests chunk #15, Upload Storybook to Chromatic, Test (all), Build, Enterprise build, Test (client/web), Test (client/browser), Test (client/jetbrains), Build TS, ESLint (all), Stylelint (all)
- **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
- **DB backcompat tests**: Backcompat test (all), Backcompat test (all (gRPC)), Backcompat test (enterprise/internal/insights), Backcompat test (enterprise/internal/insights (gRPC)), Backcompat test (internal/repos), Backcompat test (internal/repos (gRPC)), Backcompat test (enterprise/internal/batches), Backcompat test (enterprise/internal/batches (gRPC)), Backcompat test (cmd/frontend), Backcompat test (cmd/frontend (gRPC)), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Backcompat test (dev/sg), Backcompat test (dev/sg (gRPC)), Backcompat test (internal/database), Backcompat test (enterprise/internal/database)
- **CI script tests**: test-trace-command.sh
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph QA, Sourcegraph Cluster (deploy-sourcegraph) QA, Sourcegraph Upgrade
- **Publish images**: alpine-3.14, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, jaeger-agent, jaeger-all-in-one, blobstore, blobstore2, node-exporter, postgres-12-alpine, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, symbols, syntax-highlighter, worker, migrator, executor, executor-vm, batcheshelper, opentelemetry-collector, server, sg, Publish executor image, Publish executor binary, Publish docker registry mirror image
- Upload build trace

### Release branch

The run type for branches matching `^[0-9]+\.[0-9]+$` (regexp match).

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build alpine-3.14, Build cadvisor, Build codeinsights-db, Build codeintel-db, Build frontend, Build github-proxy, Build gitserver, Build grafana, Build indexed-searcher, Build jaeger-agent, Build jaeger-all-in-one, Build blobstore, Build blobstore2, Build node-exporter, Build postgres-12-alpine, Build postgres_exporter, Build precise-code-intel-worker, Build prometheus, Build prometheus-gcp, Build redis-cache, Build redis-store, Build redis_exporter, Build repo-updater, Build search-indexer, Build searcher, Build symbols, Build syntax-highlighter, Build worker, Build migrator, Build executor, Build executor-vm, Build batcheshelper, Build opentelemetry-collector, Build server, Build sg, Build executor image, Build executor binary, Build docker registry mirror image
- **Image security scans**: Scan alpine-3.14, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan jaeger-agent, Scan jaeger-all-in-one, Scan blobstore2, Scan node-exporter, Scan postgres-12-alpine, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan symbols, Scan syntax-highlighter, Scan worker, Scan migrator, Scan executor, Scan executor-vm, Scan batcheshelper, Scan opentelemetry-collector, Scan sg
- **Linters and static analysis**: GraphQL lint, Run sg lint
- **Client checks**: Puppeteer tests prep, Puppeteer tests for chrome extension, Puppeteer tests chunk #1, Puppeteer tests chunk #2, Puppeteer tests chunk #3, Puppeteer tests chunk #4, Puppeteer tests chunk #5, Puppeteer tests chunk #6, Puppeteer tests chunk #7, Puppeteer tests chunk #8, Puppeteer tests chunk #9, Puppeteer tests chunk #10, Puppeteer tests chunk #11, Puppeteer tests chunk #12, Puppeteer tests chunk #13, Puppeteer tests chunk #14, Puppeteer tests chunk #15, Upload Storybook to Chromatic, Test (all), Build, Enterprise build, Test (client/web), Test (client/browser), Test (client/jetbrains), Build TS, ESLint (all), Stylelint (all)
- **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
- **DB backcompat tests**: Backcompat test (all), Backcompat test (all (gRPC)), Backcompat test (enterprise/internal/insights), Backcompat test (enterprise/internal/insights (gRPC)), Backcompat test (internal/repos), Backcompat test (internal/repos (gRPC)), Backcompat test (enterprise/internal/batches), Backcompat test (enterprise/internal/batches (gRPC)), Backcompat test (cmd/frontend), Backcompat test (cmd/frontend (gRPC)), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Backcompat test (dev/sg), Backcompat test (dev/sg (gRPC)), Backcompat test (internal/database), Backcompat test (enterprise/internal/database)
- **CI script tests**: test-trace-command.sh
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph QA, Sourcegraph Cluster (deploy-sourcegraph) QA, Sourcegraph Upgrade
- **Publish images**: alpine-3.14, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, jaeger-agent, jaeger-all-in-one, blobstore, blobstore2, node-exporter, postgres-12-alpine, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, symbols, syntax-highlighter, worker, migrator, executor, executor-vm, batcheshelper, opentelemetry-collector, server, sg
- Upload build trace

### Browser extension release build

The run type for branches matching `bext/release` (exact match).

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- Stylelint (all)
- Test (client/browser)
- Puppeteer tests for chrome extension
- Test (all)
- E2E for chrome extension
- Extension release
- Extension release
- npm Release
- Upload build trace

### VS Code extension release build

The run type for branches matching `vsce/release` (exact match).

Base pipeline (more steps might be included based on branch changes):

- ESLint (all)
- Stylelint (all)
- Puppeteer tests for VS Code extension
- Extension release
- Upload build trace

### Main branch

The run type for branches matching `main` (exact match).

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build alpine-3.14, Build cadvisor, Build codeinsights-db, Build codeintel-db, Build frontend, Build github-proxy, Build gitserver, Build grafana, Build indexed-searcher, Build jaeger-agent, Build jaeger-all-in-one, Build blobstore, Build blobstore2, Build node-exporter, Build postgres-12-alpine, Build postgres_exporter, Build precise-code-intel-worker, Build prometheus, Build prometheus-gcp, Build redis-cache, Build redis-store, Build redis_exporter, Build repo-updater, Build search-indexer, Build searcher, Build symbols, Build syntax-highlighter, Build worker, Build migrator, Build executor, Build executor-vm, Build batcheshelper, Build opentelemetry-collector, Build server, Build sg, Build executor image, Build executor binary
- **Image security scans**: Scan alpine-3.14, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan jaeger-agent, Scan jaeger-all-in-one, Scan blobstore2, Scan node-exporter, Scan postgres-12-alpine, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan symbols, Scan syntax-highlighter, Scan worker, Scan migrator, Scan executor, Scan executor-vm, Scan batcheshelper, Scan opentelemetry-collector, Scan sg
- **Linters and static analysis**: GraphQL lint, Run sg lint
- **Client checks**: Puppeteer tests prep, Puppeteer tests for chrome extension, Puppeteer tests chunk #1, Puppeteer tests chunk #2, Puppeteer tests chunk #3, Puppeteer tests chunk #4, Puppeteer tests chunk #5, Puppeteer tests chunk #6, Puppeteer tests chunk #7, Puppeteer tests chunk #8, Puppeteer tests chunk #9, Puppeteer tests chunk #10, Puppeteer tests chunk #11, Puppeteer tests chunk #12, Puppeteer tests chunk #13, Puppeteer tests chunk #14, Puppeteer tests chunk #15, Upload Storybook to Chromatic, Test (all), Build, Enterprise build, Test (client/web), Test (client/browser), Test (client/jetbrains), Build TS, ESLint (all), Stylelint (all)
- **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
- **DB backcompat tests**: Backcompat test (all), Backcompat test (all (gRPC)), Backcompat test (enterprise/internal/insights), Backcompat test (enterprise/internal/insights (gRPC)), Backcompat test (internal/repos), Backcompat test (internal/repos (gRPC)), Backcompat test (enterprise/internal/batches), Backcompat test (enterprise/internal/batches (gRPC)), Backcompat test (cmd/frontend), Backcompat test (cmd/frontend (gRPC)), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Backcompat test (dev/sg), Backcompat test (dev/sg (gRPC)), Backcompat test (internal/database), Backcompat test (enterprise/internal/database)
- **CI script tests**: test-trace-command.sh
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph QA, Sourcegraph Cluster (deploy-sourcegraph) QA, Sourcegraph Upgrade
- **Publish images**: alpine-3.14, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, jaeger-agent, jaeger-all-in-one, blobstore, blobstore2, node-exporter, postgres-12-alpine, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, symbols, syntax-highlighter, worker, migrator, executor, executor-vm, batcheshelper, opentelemetry-collector, server, sg, Publish executor image, Publish executor binary
- Upload build trace

### Main dry run

The run type for branches matching `main-dry-run/`.
You can create a build of this run type for your changes using:

```sh
sg ci build main-dry-run
```

Base pipeline (more steps might be included based on branch changes):

- **Metadata**: Pipeline metadata
- **Pipeline setup**: Trigger async
- **Image builds**: Build alpine-3.14, Build cadvisor, Build codeinsights-db, Build codeintel-db, Build frontend, Build github-proxy, Build gitserver, Build grafana, Build indexed-searcher, Build jaeger-agent, Build jaeger-all-in-one, Build blobstore, Build blobstore2, Build node-exporter, Build postgres-12-alpine, Build postgres_exporter, Build precise-code-intel-worker, Build prometheus, Build prometheus-gcp, Build redis-cache, Build redis-store, Build redis_exporter, Build repo-updater, Build search-indexer, Build searcher, Build symbols, Build syntax-highlighter, Build worker, Build migrator, Build executor, Build executor-vm, Build batcheshelper, Build opentelemetry-collector, Build server, Build sg, Build executor image, Build executor binary
- **Image security scans**: Scan alpine-3.14, Scan cadvisor, Scan codeinsights-db, Scan codeintel-db, Scan frontend, Scan github-proxy, Scan gitserver, Scan grafana, Scan indexed-searcher, Scan jaeger-agent, Scan jaeger-all-in-one, Scan blobstore2, Scan node-exporter, Scan postgres-12-alpine, Scan postgres_exporter, Scan precise-code-intel-worker, Scan prometheus, Scan prometheus-gcp, Scan redis-cache, Scan redis-store, Scan redis_exporter, Scan repo-updater, Scan search-indexer, Scan searcher, Scan symbols, Scan syntax-highlighter, Scan worker, Scan migrator, Scan executor, Scan executor-vm, Scan batcheshelper, Scan opentelemetry-collector, Scan sg
- **Linters and static analysis**: GraphQL lint, Run sg lint
- **Client checks**: Puppeteer tests prep, Puppeteer tests for chrome extension, Puppeteer tests chunk #1, Puppeteer tests chunk #2, Puppeteer tests chunk #3, Puppeteer tests chunk #4, Puppeteer tests chunk #5, Puppeteer tests chunk #6, Puppeteer tests chunk #7, Puppeteer tests chunk #8, Puppeteer tests chunk #9, Puppeteer tests chunk #10, Puppeteer tests chunk #11, Puppeteer tests chunk #12, Puppeteer tests chunk #13, Puppeteer tests chunk #14, Puppeteer tests chunk #15, Upload Storybook to Chromatic, Test (all), Build, Enterprise build, Test (client/web), Test (client/browser), Test (client/jetbrains), Build TS, ESLint (all), Stylelint (all)
- **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
- **DB backcompat tests**: Backcompat test (all), Backcompat test (all (gRPC)), Backcompat test (enterprise/internal/insights), Backcompat test (enterprise/internal/insights (gRPC)), Backcompat test (internal/repos), Backcompat test (internal/repos (gRPC)), Backcompat test (enterprise/internal/batches), Backcompat test (enterprise/internal/batches (gRPC)), Backcompat test (cmd/frontend), Backcompat test (cmd/frontend (gRPC)), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Backcompat test (dev/sg), Backcompat test (dev/sg (gRPC)), Backcompat test (internal/database), Backcompat test (enterprise/internal/database)
- **CI script tests**: test-trace-command.sh
- **Integration tests**: Backend integration tests (gRPC), Backend integration tests, Code Intel QA
- **End-to-end tests**: Executors E2E, Sourcegraph E2E, Sourcegraph QA, Sourcegraph Cluster (deploy-sourcegraph) QA, Sourcegraph Upgrade
- **Publish images**: alpine-3.14, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, jaeger-agent, jaeger-all-in-one, blobstore, blobstore2, node-exporter, postgres-12-alpine, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, symbols, syntax-highlighter, worker, migrator, executor, executor-vm, batcheshelper, opentelemetry-collector, server, sg
- Upload build trace

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
- **Image builds**: Build alpine-3.14, Build cadvisor, Build codeinsights-db, Build codeintel-db, Build frontend, Build github-proxy, Build gitserver, Build grafana, Build indexed-searcher, Build jaeger-agent, Build jaeger-all-in-one, Build blobstore, Build blobstore2, Build node-exporter, Build postgres-12-alpine, Build postgres_exporter, Build precise-code-intel-worker, Build prometheus, Build prometheus-gcp, Build redis-cache, Build redis-store, Build redis_exporter, Build repo-updater, Build search-indexer, Build searcher, Build symbols, Build syntax-highlighter, Build worker, Build migrator, Build executor, Build executor-vm, Build batcheshelper, Build opentelemetry-collector, Build server, Build sg
- **Publish images**: alpine-3.14, cadvisor, codeinsights-db, codeintel-db, frontend, github-proxy, gitserver, grafana, indexed-searcher, jaeger-agent, jaeger-all-in-one, blobstore, blobstore2, node-exporter, postgres-12-alpine, postgres_exporter, precise-code-intel-worker, prometheus, prometheus-gcp, redis-cache, redis-store, redis_exporter, repo-updater, search-indexer, searcher, symbols, syntax-highlighter, worker, migrator, executor, executor-vm, batcheshelper, opentelemetry-collector, server, sg
- Upload build trace

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
- Upload build trace

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
- **Linters and static analysis**: Run sg lint
- **Go checks**: Test (all), Test (all (gRPC)), Test (enterprise/internal/insights), Test (enterprise/internal/insights (gRPC)), Test (internal/repos), Test (internal/repos (gRPC)), Test (enterprise/internal/batches), Test (enterprise/internal/batches (gRPC)), Test (cmd/frontend), Test (cmd/frontend (gRPC)), Test (enterprise/cmd/frontend/internal/batches/resolvers), Test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Test (dev/sg), Test (dev/sg (gRPC)), Test (internal/database), Test (enterprise/internal/database), Build
- **DB backcompat tests**: Backcompat test (all), Backcompat test (all (gRPC)), Backcompat test (enterprise/internal/insights), Backcompat test (enterprise/internal/insights (gRPC)), Backcompat test (internal/repos), Backcompat test (internal/repos (gRPC)), Backcompat test (enterprise/internal/batches), Backcompat test (enterprise/internal/batches (gRPC)), Backcompat test (cmd/frontend), Backcompat test (cmd/frontend (gRPC)), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers), Backcompat test (enterprise/cmd/frontend/internal/batches/resolvers (gRPC)), Backcompat test (dev/sg), Backcompat test (dev/sg (gRPC)), Backcompat test (internal/database), Backcompat test (enterprise/internal/database)
- Upload build trace
