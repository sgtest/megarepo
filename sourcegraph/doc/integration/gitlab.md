# GitLab integration with Sourcegraph

You can use Sourcegraph with [GitLab.com](https://github.com) and GitLab CE/EE.

Feature | Supported?
------- | ----------
[Repository syncing](../admin/external_service/gitlab.md#repository-syncing) | ✅
[Repository permissions](../admin/external_service/gitlab.md#repository-permissions) | ✅
[User authentication](../admin/external_service/gitlab.md#user-authentication) | ✅
[Browser extension](#browser-extension) | ✅

## Repository syncing

Site admins can [add GitLab repositories to Sourcegraph](../admin/external_service/gitlab.md#repository-syncing).

## Repository permissions

Site admins can [configure Sourcegraph to respect GitLab repository access permissions](../admin/external_service/gitlab.md#repository-permissions).

## User authentication

Site admins can [configure Sourcegraph to allow users to sign in via GitLab](../admin/external_service/gitlab.md#user-authentication).

## Browser extension

The [Sourcegraph browser extension](browser_extension.md) supports GitLab. When installed in your web browser, it adds hover tooltips, go-to-definition, find-references, and code search to files and merge requests viewed on GitLab.

1.  Install the [Sourcegraph browser extension](browser_extension.md).
1.  [Configure the browser extension](browser_extension.md#configuring-the-sourcegraph-instance-to-use) to use your Sourcegraph instance.

- You can also use [`https://sourcegraph.com`](https://sourcegraph.com) for public code from GitLab.com only.

1.  Click the Sourcegraph icon in the browser toolbar to open the settings page. If a permissions notice is displayed, click **Grant permissions** to allow the browser extension to work on your GitLab instance.
1.  Visit any file or merge request on GitLab. Hover over code or click the "View file" and "View repository" buttons.

![Sourcegraph for GitLab](https://cl.ly/7916fe1453a4/download/sourcegraph-for-gitLab.gif)
