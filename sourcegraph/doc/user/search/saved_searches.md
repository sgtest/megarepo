# Saved searches

Saved searches lets you save and describe search queries so you can easily monitor the results on an ongoing basis. You can create a saved search for anything, including diffs and commits across all branches of your repositories.

Saved searches can be an early warning system for common problems in your code--and a way to monitor best practices, the progress of refactors, etc. Alerts for saved searches can be sent through email or Slack, ensuring you're aware of important code changes.

---

## Examples of useful saved searches

- Recent security-related changes on all branches:
  - **Query:** `type:diff repo:@*refs/heads/ after:"5 days ago" \b(auth[^o][^r]|security\b|cve|password|secure|unsafe|perms|permissions)`
- Admitted hacks and TODOs in code:
  - **Query:** `-file:\.(json|md|txt)$ hack|todo|kludge`
- New usages of a specific function (change `MYFUNC`):
  - **Query:** `type:diff after:"1 week ago" MYFUNC\(`
- Lint-disable on unmerged branches (customize for your own linters):
  - **Query:** `repo:@*refs/heads/:^master type:diff after:"1 week ago" (tslint:disable)`

#### Discover built-in searches

Sourcegraph comes with multiple built-in searches that you can use. This includes searches for code committed with copyleft (GPL) licenses, security and authentication changes, potential secrets, API tokens and passwords, as well as various language-specific searches such as a TypeScript/JavaScript lint search to detect React `setState` race conditions.

You can find built-in searches by clicking on the **Searches** top navigation bar link, and then pressing the **Discover built-in searches** button. Simply press **Save** on any built-in search that look useful to you and follow the form to start monitoring your codebase.

---

## Creating saved searches

A saved search consists of a description and a query, both of which you can define and edit.

Saved searches are written as JSON entries in settings, and they can be associated with a user or an org:

- User saved searches are only visible to (and editable by) the user that created them.
- Org saved searches are visible to (and editable by) all members of the org.

To create a saved search:

1.  Go to **User menu > Saved searches** in the top navigation bar.
1.  Press the **+ Add new search** button.
1.  In the **Query** field, type in the components of the search query.
1.  In the **Description** field, type in a human-readable description for your saved search.
1.  In the user and org dropdown menu, select where you'd like the search to be saved.
1.  Click **Create**. The saved search is created, and you can see the number of results.

Alternatively, to create a saved search from a search you've already run:

1.  Execute a search from the homepage or navigation bar.
1.  Press the **Save this search query** button that appears on the right side of the screen above the first result.
1.  Follow the instructions from above to fill in the remaining fields.

To view saved searches, go to **User menu > Saved searches** in the top navigation bar.

---

## Configuring email and Slack notifications

Sourcegraph can automatically run your saved searches and notify you when new results are available via email and/or Slack. With this feature you can get notified about issues in your code (such as licensing issues, security changes, potential secrets being committed, etc.)

To configure email or Slack notifications, click **Edit** on a saved search and check the **Email notifications** or **Slack notifications** checkbox and press **Save**. You will receive a notification telling you it is set up and working almost instantly!

### Advanced notification configuration

By default, email notifications notify the owner of the configuration (either a single user or the entire org). Slack notifications notify an entire org (via its configured Slack webhook).

However, it is possible to create more advanced configurations, by using the following options in saved searches section of the user or org configuration:

1.  `notify` (same as **Email notifications** checkbox), whether or not to notify the configuration owner (single user or entire org) via email.
1.  `notifySlack` (same as **Slack notifications** checkbox), whether or not orgs that are notified will be notified via their configured Slack webhook.
1.  `notifyUsers`, a list of usernames (e.g. `["kim", "bob", "sarah"]`) to be explicitly notified via email. For example, the search could be saved to your org so others could see it on the Saved Searches page but only notify you (whereas `"notify": true` would notify the entire org).
1.  `notifyOrganizations`, a list of organization names (e.g.`["dev", "security", "management"]`) to be explicitly notified via email (and Slack if `notifySlack` is `true`). For example, a saved search that reveals authentication code changes could notify the entire `"dev"` and `"security"` orgs, whereas a saved search that reveals potential API secrets could notify the `"security"` and `"management"` orgs.

With the last two options above (`notifyUsers` and `notifyOrganizations`) you get a great degree of control over who is notified for a saved search -- regardless of who the owner of it is.

---
