# Inference of auto-indexing jobs

<aside class="experimental">
<p>
<span class="badge badge-experimental">Experimental</span> This feature is experimental and might change or be removed in the future. We've released it as an experimental feature to provide a preview of functionality we're working on.
</p>

<p><b>We're very much looking for input and feedback on this feature.</b> You can either <a href="https://about.sourcegraph.com/contact">contact us directly</a>, <a href="https://github.com/sourcegraph/sourcegraph">file an issue</a>, or <a href="https://twitter.com/sourcegraph">tweet at us</a>.</p>
</aside>

When a commit of a repository is selected as a candidate for [auto-indexing](./auto_indexing.md) but does not have an explicitly supplied index job configuration, index jobs are inferred from the content of the repository at that commit.

This document describes the heuristics used to determine the set of index jobs to schedule. See [configuration reference](../references/auto_indexing_configuration.md) for additional documentation on how index jobs are configured.

As a general rule of thumb, an LSIF indexer can be invoked successfully if the source code to index can be compiled successfully. The heuristics below attempt to cover the common cases of dependency resolution, but may not be sufficient if the target code requires additional steps such as code generation, header file linking, or installation of system dependencies to compile from a fresh clone of the repository. For such cases, we recommend using the inferred job as a starting point to [explicitly supply index job configuration](../how-to/configure_auto_indexing.md#explicit-index-job-configuration).

## Go

For each directory containing a `go.mod` file, the following index job is scheduled.

```yaml
indexing_jobs:
  - steps:
      - root: <dir>
        image: sourcegraph/lsif-go
        commands:
          - go mod download
    root: <dir>
    indexer: sourcegraph/lsif-go
    indexer_args:
      - lsif-go
      - --no-animation
```

For every _other_ directory excluding `vendor/` directories and their children containing one or more `*.go` files, the following index job is scheduled.

```yaml
indexing_jobs:
  - root: <dir>
    indexer: sourcegraph/lsif-go
    indexer_args:
      - GO111MODULE=off
      - lsif-go
      - --no-animation
```

## TypeScript

For each directory excluding `node_modules/` directories and their children containing a `tsconfig.json` file, the following index job is scheduled. Note that there are a dynamic number of pre-indexing steps used to resolve dependencies: for each ancestor directory `ancestor(dir)` containing a `package.json` file, the dependencies are installed via either `yarn` or `npm`. These steps run in order, depth-first.

```yaml
indexing_jobs:
  - steps:
      - root: <ancestor(dir)>
        image: sourcegraph/lsif-node:autoindex
        commands:
          # Yarn is used to resolve dependencies in an ancestor directory
          # when lerna.json configuration specifies "yarn" as the npmClient
          # or if the directory contains a yarn.lock file.
          - yarn --ignore-engines
      - root: <ancestor(dir)>
        image: sourcegraph/lsif-node:autoindex
        commands:
          # npm is used to resolve dependencies otherwise.
          - npm install
      - ...
    local_steps:
      # This is run directly before indexing if a node version can be determined
      # from the package.json "engines" field, or any of the files:
      #   - .nvmrc
      #   - .node-version
      #   - .n-node-version
      - N_NODE_MIRROR=https://unofficial-builds.nodejs.org/download/release n --arch x64-musl autol
    root: <dir>
    indexer: sourcegraph/lsif-node:autoindex
    indexer_args:
      - lsif-tsc
      - -p
      - .
```

## Rust

If the repository contains a `Cargo.toml` file, the following index job is scheduled.

```yaml
indexing_jobs:
  - root: ''
    indexer: sourcegraph/lsif-rust
    indexer_args:
      - lsif-rust
      - index
    outfile: dump.lsif
```

## Java

> NOTE: Inference for languages supported by [lsif-java](https://github.com/sourcegraph/lsif-java) is currently restricted to Sourcegraph Cloud.

If the repository contains both a `lsif-java.json` file as well as `*.java`, `*.scala`, or `*.kt` files, the following index job is scheduled.

```yaml
indexing_jobs:
  - root: ''
    indexer: sourcegraph/lsif-java
    indexer_args:
      - lsif-java
      - index
      - --build-tool=lsif
    outfile: dump.lsif
```
