# Sourcegraph documentation

Sourcegraph is a code search and browsing tool with code intelligence that helps developers write and review code. Learn more about Sourcegraph at [about.sourcegraph.com](https://about.sourcegraph.com) and use it at [Sourcegraph.com](https://sourcegraph.com).

Sourcegraph development is open source at [github.com/sourcegraph/sourcegraph](https://github.com/sourcegraph/sourcegraph). Need help? Use the [issue tracker](https://github.com/sourcegraph/sourcegraph/issues).

## Quickstart

Run a self-hosted Sourcegraph instance in 1 command:

```
docker run \
  --publish 7080:7080 --publish 2633:2633 --rm \
  --volume ~/.sourcegraph/config:/etc/sourcegraph \
  --volume ~/.sourcegraph/data:/var/opt/sourcegraph \
  --volume /var/run/docker.sock:/var/run/docker.sock \
  sourcegraph/server:2.13.5
```

Continue at http://localhost:7080, and see [administrator documentation](admin/index.md) for next steps.

Add code intelligence (hover tooltips, jump-to-definition, find-references) for languages like [Go](https://sourcegraph.com/extensions/sourcegraph/lang-go), [TypeScript](https://sourcegraph.com/extensions/sourcegraph/lang-typescript), [Python](https://sourcegraph.com/extensions/sourcegraph/python), and [others](https://sourcegraph.com/extensions?query=category%3A%22Programming+languages%22) by enabling the corresponding [Sourcegraph extension](extensions/index.md) on the [Sourcegraph extension registry](https://sourcegraph.com/extensions).

## Overview

### Core documentation

- [**User documentation**](user/index.md)
- [**Administrator documentation**](admin/index.md)
- [Install Sourcegraph](admin/install/index.md)
- [Update Sourcegraph](admin/updates.md)
- [Sourcegraph extensions](extensions/index.md)
- [Contributing to Sourcegraph](dev/index.md)
- [Sourcegraph Enterprise](admin/subscriptions/index.md)
- [Sourcegraph roadmap](dev/roadmap.md)

### Features and tutorials

- [Overview](user/index.md): What is Sourcegraph?
- [Tour](user/tour.md): A walkthrough of Sourcegraph's features, with real-world example use cases.
- [Chrome and Firefox browser extensions](integration/browser_extension.md)
- [Query syntax reference](user/search/queries.md)
- [Building a Sourcegraph extension](extensions/authoring/index.md) to add features and integrations to Sourcegraph
- [Code search](user/search/index.md)
- [Code intelligence](user/code_intelligence/index.md)
- [Other integrations](integration/index.md)
- [GraphQL API](api/graphql.md)

<!-- TODO(sqs): Add link to ./graphbook when it has more content. -->

## Sourcegraph subscriptions

You can use Sourcegraph in 2 ways:

- [Self-hosted Sourcegraph](admin/install/index.md): Deploy and manage your own Sourcegraph instance.
- [Sourcegraph.com](https://sourcegraph.com): For public code only. No signup or installation required.

For self-hosted Sourcegraph instances, you run a Docker image or Kubernetes cluster on-premises or on your preferred cloud provider. There are [3 tiers](https://about.sourcegraph.com/pricing): Core (free), Enterprise Starter, and Enterprise. Enterprise features require a [Sourcegraph subscription](https://sourcegraph.com/user/subscriptions).

## Other links

- [Sourcegraph open-source repository](https://github.com/sourcegraph/sourcegraph)
- [Sourcegraph blog](https://about.sourcegraph.com/blog/)
- [Issue tracker](https://github.com/sourcegraph/sourcegraph/issues)
- [about.sourcegraph.com](https://about.sourcegraph.com) (general information about Sourcegraph)
- [@srcgraph on Twitter](https://twitter.com/srcgraph)
