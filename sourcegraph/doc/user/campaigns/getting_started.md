# Getting started

Follow these steps to get started with campaigns on your Sourcegraph instance:

1. **Enable the Campaigns feature flag**: [Configuration](./configuration.md). Since campaigns are currently in beta, they're behind a feature flag and need to be enabled by a site admin.
1. **Setup the `src` CLI on your machine**: [Installation and setup instructions](https://github.com/sourcegraph/src-cli/#installation)

Now you're ready to [create a manual campaign](./creating_manual_campaign.md) or to [create a campaign from patches](./creating_campaign_from_patches.md).

---

It's optional, but we **highly recommended to setup webhook integration** on your Sourcegraph instance for optimal syncing performance between your code host and Sourcegraph.

* GitHub: [Configuring GitHub webhooks](https://docs.sourcegraph.com/admin/external_service/github#webhooks).
* Bitbucket Server: [Setup the `bitbucket-server-plugin`](https://github.com/sourcegraph/bitbucket-server-plugin), [create a webhook](https://github.com/sourcegraph/bitbucket-server-plugin/blob/master/src/main/java/com/sourcegraph/webhook/README.md#create) and configure the `"plugin"` settings for your [Bitbucket Server code host connection](https://docs.sourcegraph.com/admin/external_service/bitbucket_server#configuration).
