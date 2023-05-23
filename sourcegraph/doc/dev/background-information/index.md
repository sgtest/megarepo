<!-- Link back any new sections to doc/dev/index.md as well -->

# Background information

## Overview

- [Tech stack](tech_stack.md)
- [Security Patterns](security_patterns.md)

## [Architecture](architecture/index.md)

- [Overview](architecture/index.md)
- [Introducing a new service](architecture/introducing_a_new_service.md)

## [Sourcegraph App](app/index.md)

- [Notes about code signing the Sourcegraph App](./app/codesigning.md)

## Development

- [`sg` - the Sourcegraph developer tool](./sg/index.md)
  - [Full `sg` reference](./sg/reference.md)
- [Using Bazel](./bazel.md)
  - [Bazel and client code](./bazel_web.md)
- [Developing the web clients](web/index.md)
  - [Developing the web app](web/web_app.md)
  - [Developing the code host integrations](web/code_host_integrations.md)
  - [Working with GraphQL](web/graphql.md)
  - [Wildcard Component Library](web/wildcard.md)
  - [Styling UI](web/styling.md)
  - [Accessibility](web/accessibility/index.md)
  - [Temporary settings](web/temporary_settings.md)
  - [Build process](web/build.md)
- [Developing the GraphQL API](graphql_api.md)
- [Developing the SCIM API](scim_api.md)
- [Developing batch changes](batch_changes/index.md)
- [Developing code intelligence](codeintel/index.md)
- [Developing code insights](insights/index.md)
- [Developing code monitoring](codemonitoring/index.md)
- [Developing observability](observability/index.md)
- [Dependencies and generated code](dependencies_and_codegen.md)
- [Pull request reviews](pull_request_reviews.md)
- [Commit messages](commit_messages.md)
- [Exposing services](exposing-services.md)
- [Developing a store](basestore.md)
- [Developing a worker](workers.md)
- [Developing an out-of-band migration](oobmigrations.md)
- [Developing a background routine](backgroundroutine.md)
- [Building p4-fusion](./build_p4_fusion.md)
- [The `gitserver` API](./gitserver-api.md)

## Git

- [`git gc` and its modes of operations in Sourcegraph](./git_gc.md)

## [Languages](languages/index.md)

- [Go](languages/go.md)
  - [Error handling in Go](languages/go_errors.md)
- [TypeScript](languages/typescript.md)
- [Bash](languages/bash.md)
- [Terraform](languages/terraform.md)

## [SQL](sql/index.md)

- [Migrations overview](sql/migrations_overview.md)
- [Migrations](sql/migrations.md)
- High-performance guides
  - [Batch operations](sql/batch_operations.md)
  - [Materialized cache](sql/materialized_cache.md)

## Testing

- [Continuous Integration](ci/index.md)
- [Testing a pull request](testing_pr.md)
- [Testing Principles](testing_principles.md)
- [Testing Go code](languages/testing_go_code.md)
- [Testing web code](testing_web_code.md)

## Tools

- [Renovate dependency updates](renovate.md)
- [Honeycomb](honeycomb.md)
- [Using PostgreSQL](postgresql.md)

## Wolfi

- [Wolfi Overview](wolfi/index.md)

## Other

- [Telemetry](telemetry.md)
- [Adding, changing and debugging pings](adding_ping_data.md)
- [Deploy Sourcegraph with Helm chart (BETA)](../../admin/deploy/kubernetes/helm.md)
- [Event level data usage pipeline](data-usage-pipeline.md)
- [Adding, changing and debugging user event data](adding_event_level_data.md)
- [GitHub API Oddities](github-api-oddities.md)
