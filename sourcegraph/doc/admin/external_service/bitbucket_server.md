# Bitbucket Server / Bitbucket Data Center

Site admins can sync Git repositories hosted on [Bitbucket Server](https://www.atlassian.com/software/bitbucket/server) or [Bitbucket Data Center](https://www.atlassian.com/enterprise/data-center/bitbucket) with Sourcegraph so that users can search and navigate their repositories.

To connect Bitbucket Server / Bitbucket Data Center to Sourcegraph:

1. Go to **Site admin > Manage code hosts > Add repositories**
1. Select **Bitbucket Server / Bitbucket Data Center**.
1. Configure the connection to Bitbucket Server / Bitbucket Data Center using the action buttons above the text field, and additional fields can be added using <kbd>Cmd/Ctrl+Space</kbd> for auto-completion. See the [configuration documentation below](#configuration).
1. Press **Add repositories**.

Also consider installing the [Sourcegraph Bitbucket Server plugin](../../integration/bitbucket_server.md#sourcegraph-bitbucket-server-plugin) which enables native code navigation for every Bitbucket user when browsing code and reviewing pull requests, allows for faster permission syncing between Sourcegraph and Bitbucket Server / Bitbucket Data Center and adds support for webhooks to Bitbucket Server / Bitbucket Data Center.

## Access token permissions

Sourcegraph requires a Bitbucket Server / Bitbucket Data Center personal access token with **read** permissions to sync repositories.

When using [batch changes](../../batch_changes/index.md) the access token needs **write** permissions on the project and repository level. See "[Code host interactions in batch changes](../../batch_changes/explanations/permissions_in_batch_changes.md#code-host-interactions-in-batch-changes)" for details.

You can create a personal access token at `https://[your-bitbucket-hostname]/plugins/servlet/access-tokens/add`. Also set the corresponding `username` field.

For Bitbucket Server instances that don't support personal access tokens (Bitbucket Server version 5.4 and older), specify user-password credentials in the `username` and `password` fields.

## Repository syncing

There are four fields for configuring which repositories are mirrored:

- [`repos`](bitbucket_server.md#configuration)<br>A list of repositories in `projectKey/repositorySlug` format. The order determines the order in which we sync repository metadata and is safe to change.
- [`repositoryQuery`](bitbucket_server.md#configuration)<br>A list of strings with some pre-defined options (`none`, `all`), and/or a [Bitbucket Server / Bitbucket Data Center Repo Search Request Query Parameters](https://docs.atlassian.com/bitbucket-server/rest/6.1.2/bitbucket-rest.html#idp355).
- [`exclude`](bitbucket_server.md#configuration)<br>A list of repositories to exclude which takes precedence over the `repos`, and `repositoryQuery` fields.
- [`excludePersonalRepositories`](bitbucket_server.md#configuration)<br>With this enabled, Sourcegraph will exclude any personal repositories from being imported, even if it has access to them.

## Webhooks

Using the `webhooks` property on the external service has been deprecated.

Please consult [this page](../config/webhooks.md) in order to configure webhooks.

## Repository permissions

Enforcing Bitbucket Server / Bitbucket Data Center permissions can be configured via the `authorization` setting in its configuration.

> NOTE: It can take some time to complete full cycle of repository permissions sync if you have a large number of users or repositories. [See sync duration time](../permissions/syncing.md#sync-duration) for more information.

### Prerequisites

1. You have the exact same user accounts, **with matching usernames**, in Sourcegraph and Bitbucket Server / Bitbucket Data Center. This can be accomplished by configuring an [external authentication provider](../auth/index.md) that mirrors user accounts from a central directory like LDAP or Active Directory. The same should be done on Bitbucket Server / Bitbucket Data Center with [external user directories](https://confluence.atlassian.com/bitbucketserver/external-user-directories-776640394.html).
1. Ensure you have set `auth.enableUsernameChanges` to **`false`** in the [site config](../config/site_config.md) to prevent users from changing their usernames and **escalating their privileges**.


### Setup

This section walks you through the process of setting up an *Application Link between Sourcegraph and Bitbucket Server / Bitbucket Data Center* and configuring the Sourcegraph Bitbucket Server / Bitbucket Data Center configuration with `authorization` settings. It assumes the above prerequisites are met.

As an admin user, go to the "Application Links" page. You can use the sidebar navigation in the admin dashboard, or go directly to [https://bitbucketserver.example.com/plugins/servlet/applinks/listApplicationLinks](https://bitbucketserver.example.com/plugins/servlet/applinks/listApplicationLinks).

<img src="https://imgur.com/Hg4bzOf.png" width="800">

Write Sourcegraph's external URL in the text area (e.g. `https://sourcegraph.example.com`) and click **Create new link**. Click **Continue** even if Bitbucket Server / Bitbucket Data Center warns you about the given URL not responding.

<img src="https://imgur.com/x6vFKIL.png" width="800">

Write `Sourcegraph` as the *Application Name* and select `Generic Application` as the *Application Type*. Leave everything else unset and click **Continue**.

<img src="https://imgur.com/161rbB9.png" width="800">

Now click the edit button in the `Sourcegraph` Application Link that you just created and select the `Incoming Authentication` panel.

<img src="https://imgur.com/sMGmzhH.png" width="800">

Generate a *Consumer Key* in your terminal with `echo sourcegraph$(openssl rand -hex 16)`. Copy this command's output and paste it in the *Consumer Key* field. Write `Sourcegraph` in the *Consumer Name* field.

<img src="https://imgur.com/1kK2Y5x.png" width="800">

Generate an RSA key pair in your terminal with `openssl genrsa -out sourcegraph.pem 4096 && openssl rsa -in sourcegraph.pem -pubout > sourcegraph.pub`. Copy the contents of `sourcegraph.pub` and paste them in the *Public Key* field.

<img src="https://imgur.com/YHm1uSr.png" width="800">

Scroll to the bottom and check the *Allow 2-Legged OAuth* checkbox, then write your admin account's username in the *Execute as* field and, lastly, check the *Allow user impersonation through 2-Legged OAuth* checkbox. Press **Save**.

<img src="https://imgur.com/1qxEAye.png" width="800">

Go to your Sourcegraph's *Manage code hosts* page (i.e. `https://sourcegraph.example.com/site-admin/external-services`) and either edit or create a new *Bitbucket Server / Bitbucket Data Center* connection. Add the following settings:

```json
{
	// Other config goes here
	"authorization": {
		"identityProvider": {
			"type": "username"
		},
		"oauth": {
			"consumerKey": "<KEY GOES HERE>",
			"signingKey": "<KEY GOES HERE>"
		}
	}
}
```

Copy the *Consumer Key* you generated before to the `oauth.consumerKey` field and the output of the command `base64 sourcegraph.pem | tr -d '\n'` to the `oauth.signingKey` field. Finally, **save the configuration**. You're done!

### Fast permission sync with Bitbucket Server plugin

By installing the [Bitbucket Server plugin](../../../integration/bitbucket_server.md), you can make use of the fast permission sync feature that allows using Bitbucket Server / Bitbucket Data Center permissions on larger instances.

### Fast permission syncing

With the [Sourcegraph Bitbucket Server](../../integration/bitbucket_server.md#sourcegraph-bitbucket-server-plugin) you can enable fast permission syncing:

1. Connect Bitbucket Server / Bitbucket Data Center to Sourcegraph (_see instructions above_).
1. Follow the [instructions to set up repository permissions](#repository-permissions) with Bitbucket Server / Bitbucket Data Center.
1. Install the [Sourcegraph Bitbucket Server plugin](../../integration/bitbucket_server.md#sourcegraph-bitbucket-server-plugin) on your Bitbucket Server / Bitbucket Data Center instance.
1. In Sourcegraph, go to **Site admin > Manage code hosts** and edit the Bitbucket Server / Bitbucket Data Center configuration.
1. Add the `"plugin.permissions"` property:

```json
{
  // [...]
  "plugin": {
    "permissions": "enabled"
  }
}
```

### Authentication for older Bitbucket Server / Bitbucket Data Center versions

Bitbucket Server / Bitbucket Data Center versions older than v5.5 require specifying a less secure username and password combination, as those versions of Bitbucket Server / Bitbucket Data Center do not support [personal access tokens](https://confluence.atlassian.com/bitbucketserver/personal-access-tokens-939515499.html).

### HTTPS cloning

Sourcegraph by default clones repositories from your Bitbucket Server / Bitbucket Data Center via HTTP(S), using the access token or account credentials you provide in the configuration. The [`username`](bitbucket_server.md#configuration) field is always used when cloning, so it is required.

## Repository labels

Sourcegraph will mark repositories as archived if they have the `archived` label on Bitbucket Server / Bitbucket Data Center. You can exclude these repositories in search with `archived:no` [search syntax](../../code_search/reference/queries.md).

## Internal rate limits

Internal rate limiting can be configured to limit the rate at which requests are made from Sourcegraph to Bitbucket Server / Bitbucket Data Center. 

If enabled, the default rate is set at 28,800 per hour (8 per second) which can be configured via the `requestsPerHour` field (see below):

- For Sourcegraph <=3.38, if rate limiting is configured more than once for the same code host instance, the most restrictive limit will be used.
- For Sourcegraph >=3.39, rate limiting should be enabled and configured for each individual code host connection.

**NOTE** Internal rate limiting is only currently applied when synchronising changesets in [batch changes](../../batch_changes/index.md), repository permissions and repository metadata from code hosts.

## Configuration

Bitbucket Server / Bitbucket Data Center connections support the following configuration options, which are specified in the JSON editor in the site admin "Manage code hosts" area.

<div markdown-func=jsonschemadoc jsonschemadoc:path="admin/external_service/bitbucket_server.schema.json">[View page on docs.sourcegraph.com](https://docs.sourcegraph.com/admin/external_service/bitbucket_server) to see rendered content.</div>
