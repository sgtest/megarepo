# GitHub integration with Sourcegraph

Sourcegraph integrates with GitHub and GitHub Enterprise.

## Syncing GitHub repositories

Sourcegraph supports syncing repositories from GitHub.com and GitHub Enterprise (version 2.10 and newer).

- Add GitHub as an external service (in **Site admin > External services**, or in the site config JSON editor in Sourcegraph 2.x)

- Read the [GitHub configuration documentation](../admin/site_config/all.md#githubconnection-object) or press Ctrl+Space or Cmd+Space in the configuration editor.

By default, it adds all repositories that are affiliated with the user whose token you provide.

If you want to synchronize public repositories from GitHub.com, the repositories need to be explicitly enumerated in the [repos](https://docs.sourcegraph.com/admin/site_config/all#repos-array) field of the GitHub external service configuration.

If you don't want to use an access token from your personal GitHub user account, generate a token for a [machine user](https://developer.github.com/v3/guides/managing-deploy-keys/#machine-users) affiliated with the organizations whose repositories you wish to make available.

**GitHub.com rate limits**

You should always include a token in a configuration for a GitHub.com URL to avoid being denied service by GitHub's [unauthenticated rate limits](https://developer.github.com/v3/#rate-limiting). If you don't want to automatically synchronize repositories from the account associated with your personal access token, you can create a token without a [repo scope](https://developer.github.com/apps/building-oauth-apps/scopes-for-oauth-apps/#available-scopes) for the purposes of bypassing rate limit restrictions only.

## Authentication

To configure GitHub as an authentication provider (which will enable sign-in via GitHub), see the
[authentication documentation](../admin/auth.md#github).

## Repository permissions

By default, all Sourcegraph users can view all repositories. To configure Sourcegraph to use
GitHub's per-user repository permissions, see "[Repository
permissions](../admin/repo/permissions.md#github)".

## Browser extension

The [Sourcegraph browser extension](browser_extension.md) supports GitHub. When installed in your web browser, it adds hover tooltips, go-to-definition, find-references, and code search to files and pull requests viewed on GitHub and GitHub Enterprise.

1.  Install the [Sourcegraph browser extension](browser_extension.md).
1.  [Configure the browser extension](browser_extension.md#configuring-the-sourcegraph-instance-to-use) to use your Sourcegraph instance (where you've added the `github` site config property).

- You can also use [`https://sourcegraph.com`](https://sourcegraph.com) for public code only.

1.  GitHub Enterprise only: Click the Sourcegraph icon in the browser toolbar to open the settings page. If a permissions notice is displayed, click **Grant permissions** to allow the browser extension to work on your GitHub Enterprise instance.
1.  Visit any file or pull request on GitHub. Hover over code or click the "View file" and "View repository" buttons.
