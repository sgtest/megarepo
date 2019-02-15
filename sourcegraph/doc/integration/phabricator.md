# Phabricator integration with Sourcegraph

You can use Sourcegraph with [Phabricator](http://phabricator.org/).

Feature | Supported?
------- | ----------
[Repository linking/syncing](../admin/external_service/phabricator.md#repository-linking-and-syncing) | ✅
[Repository permissions](../admin/repo/permissions.md) | ❌
[User authentication](../admin/auth.md) | ❌
[Browser extension](#browser-extension) | ✅
[Native extension](../admin/external_service/phabricator.md#native-extension) | ✅

## Repository syncing and linking

Site admins can [link and sync Phabricator repositories to Sourcegraph](../admin/external_service/phabricator.md#repository-syncing-and-linking).

## Browser extension

The [Sourcegraph browser extension](browser_extension.md) supports Phabricator. When installed in your web browser, it adds hover tooltips, go-to-definition, find-references, and code search to files and diffs viewed on Phabricator.

1.  Install the [Sourcegraph browser extension](browser_extension.md).
1.  [Configure the browser extension](browser_extension.md#configuring-the-sourcegraph-instance-to-use) to use your Sourcegraph instance.
1.  Click the Sourcegraph icon in the browser toolbar to open the settings page. If a permissions notice is displayed, click **Grant permissions** to allow the browser extension to work on your Phabricator instance.
1.  Visit any file or diff on Phabricator. Hover over code or click the "View file" and "View repository" buttons.

> NOTE: Site admins can also install the [native Phabricator extension](../admin/external_service/phabricator.md#native-extension) to avoid needing each user to install the browser extension.
