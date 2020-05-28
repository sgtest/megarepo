# Administration

Site administrators are the admins responsible for deploying, managing, and configuring Sourcegraph for regular users. They have [special privileges](privileges.md) on the Sourcegraph instance.

## [Install Sourcegraph](install/index.md)

- [Install Sourcegraph with Docker](install/docker/index.md)
- [Install Sourcegraph with Docker Compose](install/docker-compose/index.md)
- [Install Sourcegraph on a cluster](install/cluster.md)
- [Set up a managed instance](install/managed.md)

## Management, deployment, and configuration

- [Configuration](config/index.md)
- [Adding Git repositories](repo/add.md) (from a code host or clone URL)
- [HTTP and HTTPS/SSL configuration](http_https_configuration.md)
- [Management console (removed in v3.11)](management_console.md)
- [Monorepo](monorepo.md)
- [Repository webhooks](repo/webhooks.md)
- [User authentication](auth/index.md)
- [Upgrading Sourcegraph](updates.md)
- [Setting the URL for your instance](url.md)
- [Observability](observability.md)
- [Repository permissions](repo/permissions.md)
- [PostgreSQL configuration](postgres-conf.md)
- [Upgrading PostgreSQL](postgres.md)
- [Using external databases (PostgreSQL and Redis)](external_database.md)
- [User data deletion](user_data_deletion.md)

## Features

- [Code intelligence and language servers](../user/code_intelligence/index.md)
- [Sourcegraph extensions and extension registry](extensions/index.md)
- [Search](search.md)
- [Federation](federation/index.md)
- [Pings](pings.md)
- [Usage statistics](../user/usage_statistics.md)
- [User feedback surveys](../user/user_surveys.md)

## Integrations

- [GitHub and GitHub Enterprise](../integration/github.md)
- [GitLab](../integration/gitlab.md)
- [Bitbucket Server](../integration/bitbucket_server.md)
- [AWS CodeCommit](../integration/aws_codecommit.md)
- [Phabricator](../integration/phabricator.md)
- [All integrations](../integration/index.md)

## Migration guides

- [From OpenGrok to Sourcegraph](migration/opengrok.md)
- [Migrating to Sourcegraph 3.0.1+](migration/3_0.md)
- [Migrating to Sourcegraph 3.7.2+](migration/3_7.md)
- [Pricing and subscriptions](subscriptions/index.md)
- [FAQ](faq.md)
