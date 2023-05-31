# Batch Changes site admin configuration reference

Batch Changes is generally configured through the same [site configuration](site_config.md) and [code host configuration](../external_service/index.md) as the rest of Sourcegraph. However, Batch Changes features may require specific configuration, and those are documented here.

## Access control

<span class="badge badge-note">Sourcegraph 5.0+</span>

Batch Changes is [RBAC-enabled](../../admin/access_control/index.md) <span class="badge badge-beta">Beta</span>. By default, all users have full read and write access for Batch Changes, but this can be restricted by changing the default role permissions, or by creating new custom roles.

### Enable organization members to administer

<span class="badge badge-note">Sourcegraph 5.0.5+</span>

By default, only a batch change's author or a site admin can administer (apply, close, rename, etc.) a batch change. However, admins can use [organizations](../../admin/organizations.md) to facilitate closer collaboration and shared administrative control over batch changes by enabling the `orgs.allMembersBatchChangesAdmin` setting for an organization. When enabled, members of the organization will be able to administer all batch changes created in that organization's namespace. Batch changes created in other namespaces (user or organization) will still be restricted to the author and site admins.

## Rollout windows

By default, Sourcegraph attempts to reconcile (create, update, or close) changesets as quickly as the rate limits on the code host allow. This can result in CI systems being overwhelmed if hundreds or thousands of changesets are being handled as part of a single batch change.

Configuring rollout windows allows changesets to be reconciled at a slower or faster rate based on the time of day and/or the day of the week. These windows are applied to changesets across all code hosts, but they only affect the rate at which changesets are created/published, updated, or closed, as well as some other internal operations like importing and detaching. Bulk operations to publish changesets also respect the rollout window; however, bulk commenting, merging, and closing will happen all at once.

Rollout windows are configured through the `batchChanges.rolloutWindows` [site configuration option](site_config.md). If specified, this option contains an array of rollout window objects that are used to schedule changesets. The format of these objects [is given below](#rollout-window-object).

### Behavior

When rollout windows are enabled, changesets will initially enter a **Scheduled** state when their batch change is applied. Hovering or tapping on the changeset's state icon will provide an estimate of when the changeset will be reconciled.

To restore the default behavior, you can either delete the `batchChanges.rolloutWindows` option, or set it to `null`.

Or, to put it another way:

| `batchChanges.rolloutWindows` configuration | Behavior |
|---------------------------------------------|-----------|
| Omitted, or set to `null`                   | Changesets will be reconciled as fast as the code host allows; essentially the same as setting a single `{"rate": "unlimited"}` window. |
| Set to an array (even if empty)             | Changesets will be reconciled using the rate limit in the current window using [the leaky bucket behavior described below](#leaky-bucket-rate-limiting). If no window covers the current period, then no changesets will be reconciled until a window with a non-zero [`rate`](#rate) opens. |
| Any other value                             | The configuration is invalid, and an error will appear. |

#### Leaky bucket rate limiting

Rate limiting uses the [leaky bucket algorithm](https://en.wikipedia.org/wiki/Leaky_bucket) to smooth bursts in reconciliations.

Practically speaking, this means that the given rate can be thought of more as an average than as a simple resource allocation. If there are always changesets in the queue, a rate of `10/hour` means that a changeset will be reconciled approximately every six minutes, rather than ten changesets being simultaneously reconciled at the start of each hour.

### Avoiding hitting rate limits

Keep in mind that if you configure a rollout window that is too aggressive, you risk exceeding your code hosts' API rate limits. We recommend maintaining a rate that is no faster than `5/minute`; however, you can refer to your code host's API docs if you wish to increase it beyond this recommendation:

* [GitHub](https://docs.github.com/en/graphql/overview/resource-limitations#rate-limit)
* [GitLab](https://docs.gitlab.com/ee/user/gitlab_com/index.html#gitlabcom-specific-rate-limits)
* [Bitbucket Cloud](https://support.atlassian.com/bitbucket-cloud/docs/api-request-limits/)

When using a [global service account token](../../batch_changes/how-tos/configuring_credentials.md#global-service-account-tokens) with Batch Changes, keep in mind that this token will also be used for other Batch Changes <> code host interactions, too.

You may encounter this error when publishing changesets to GitHub:

> **Failed to run operations on changeset**
>
> Creating changeset: error in GraphQL response: was submitted too quickly

In addition to their normal API rate limits, GitHub also has an internal _content creation_ limit (also called [secondary rate limit](https://docs.github.com/en/rest/guides/best-practices-for-integrators?apiVersion=2022-11-28#dealing-with-secondary-rate-limits)), which is an [intentional](https://github.com/cli/cli/issues/4801#issuecomment-1029207971) restriction on the platform to combat abuse by automated actors. At the time of writing, the specifics of this limit remain undocumented, due largely to the fact that it is dynamically determined (see [this GitHub issue](https://github.com/cli/cli/issues/4801)). However, the behavior of the limit is that it only permits a fixed number of resources to be created per minute and per hour, and exceeding this limit triggers a temporary hour-long suspension during which time no additional resources of this type can be created.

Presently, Batch Changes does not automatically work around this limit (feature request tracked [here](https://github.com/sourcegraph/sourcegraph/issues/44631). The current guidance if you do encounter this issue is to wait an hour and then try again, setting a less frequent `rolloutWindows` rate until this issue is no longer encountered.

### Rollout window object

A rollout window is a JSON object that looks as follows:

```json
{
  "rate": "10/hour",
  "days": ["saturday", "sunday"],
  "start": "06:00",
  "end": "20:00"
}
```

All fields are optional except for `rate`, and are described below in more detail. All times and days are handled in UTC.

In the event multiple windows overlap, the last defined window will be used.

#### `rate`

`rate` describes the rate at which changesets will be reconciled. This may be expressed in one of the following ways:

* The string `unlimited`, in which case no limit will be applied for this window, or
* A string in the format `N/UNIT`, where `N` is a number and `UNIT` is one of `second`, `minute`, or `hour`; for example, `10/hour` would allow 10 changesets to be reconciled per hour, or
* The number `0`, which will prevent any changesets from being reconciled when this window is active.

#### `days`

`days` is an array of strings that defines the days of the week that the window applies to. English day names are accepted in a case insensitive manner:

* `["saturday", "sunday"]` constrains the window to Saturday and Sunday.
* `["tuesday"]` constrains the window to only Tuesday.

If omitted or an empty array, all days of the week will be matched.

#### `start` and `end`

`start` and `end` define the start and end of the window on each day that is matched by [`days`](#days), or every day of the week if `days` is omitted. Values are defined as `HH:MM` in UTC.

Both `start` and `end` must be provided or omitted: providing only one is invalid.

### Examples

To rate limit changeset publication to 3 per minute between 08:00 and 16:00 UTC on weekdays, and allow unlimited changesets outside of those hours:

```json
[
  {
    "rate": "unlimited"
  },
  {
    "rate": "3/minute",
    "days": ["monday", "tuesday", "wednesday", "thursday", "friday"],
    "start": "08:00",
    "end": "16:00"
  }
]
```

To only allow changesets to be reconciled at 1 changeset per minute on (UTC) weekends:

```json
[
  {
    "rate": "1/minute",
    "days": ["saturday", "sunday"]
  }
]
```

## Incoming webhooks

<span class="badge badge-note">Sourcegraph 3.33+</span>

Sourcegraph can track incoming webhooks from code hosts to more easily debug issues with webhook delivery. Learn [how to setup webhooks and configure logging](../../admin/config/webhooks/incoming.md#webhook-logging).

## Forks

<span class="badge badge-note">Sourcegraph 3.36+</span>

Sourcegraph can be configured to push branches created by Batch Changes to a fork of the repository, rather than the repository itself, by enabling the `batchChanges.enforceForks` site configuration option.

When enabled, Batch Changes will now prefix the name of the fork repo it creates with the original repo's namespace name in order to prevent repo name collisions. For example, a changeset that opens a pull request against https://github.com/org/project would push the branch to https://github.com/user/org-project. Note that if a [global service account](../../batch_changes/how-tos/configuring_credentials.md#global-service-account-tokens) is in use, then the fork will be created in the namespace of the service account, **not** the user.

You can also specify this behaivor per batch change when the property `changesetTemplate.fork` is specified in the batch spec. This will override the site configuration setting, enabling per-batch-change control for pushing to a fork.
### Examples

To enable forks, update the site configuration to include:

```json
{
  "batchChanges.enforceForks": true
}
```

## Automatically delete branches on merge/close

<span class="badge badge-note">Sourcegraph 5.1+</span>

Sourcegraph can be configured to automatically delete branches created for Batch Changes changesets when changesets are merged or closed by enabling the `batchChanges.autoDeleteBranch` site configuration option.

When enabled, Batch Changes will override any setting on the repository on the code host itself and attempt to remove the source branch of the changeset when the changeset is merged or closed. This is useful for keeping repositories clean of stale branches.

Not every code host supports this in the same way; some code host APIs expose a property on the changeset which can be toggled to enable this behavior, while others require a separate API call to delete the branch after the changeset is merged/closed.

For those that support a changeset property, Batch Changes will automatically set the property to match the site config setting. The property will be updated whenever the changeset is updated, so that the settings stay in sync.

For those that require a separate API call, Batch Changes will only be able to delete the branch if the changeset is merged/closed _using Sourcegraph_. If the changeset is merged/closed on the code host itself, Batch Changes will not be able to delete the branch.

Refer to the table below to see the levels with which each code host is supported:

Code Host | Changeset property or separate API call? | Support on merge | Support on close
--------- | --------- | :-: | :-:
Azure DevOps | Changeset property | ✓ | ✗
Bitbucket Cloud | Changeset property | ✓ | ✓
Bitbucket Server | API call | ✓ | ✓
GitHub | API call | ✓ | ✓
GitLab | Changeset property | ✓ | ✓
