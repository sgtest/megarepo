# Publishing changesets to the code host

After you've [created a campaign](creating_a_campaign.md) with `published: false` in its campaign spec, you can see a preview of the changesets (e.g., GitHub pull requests) that will be created on the code host once they're published:

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/campaigns/publishing_changesets_preview_unpublished.png" class="screenshot center">

In order to create these changesets on the code hosts, you need to publish them.

## Requirements

To publish a changeset, you need:

1. [admin permissions for the campaign](../explanations/permissions_in_campaigns.md#permission-levels-for-campaigns),
1. write access to the changeset's repository (on the code host), and
1. a personal access token [configured in Sourcegraph for your code host(s)](configuring_user_credentials.md).  

For more information, see "[Code host interactions in campaigns](../explanations/permissions_in_campaigns.md#code-host-interactions-in-campaigns)".
[Forking the repository](../explanations/introduction_to_campaigns.md#known-issues) is not yet supported.

## Publishing changesets

When you're ready, you can publish all of a campaign's changesets by changing the `published: false` in your campaign spec to `true`:

```yaml
name: hello-world

# ...

changesetTemplate:
  # ...
  published: true
```

Then run the `src campaign preview` command again, or `src campaign apply` to immediately publish the changesets.

Publishing a changesets will:

- Create a commit with the changes from the patches for that repository.
- Push a branch using the branch name you defined in the campaign spec with [`changesetTemplate.branch`](../references/campaign_spec_yaml_reference.md#changesettemplate-branch).
- Create a changeset (e.g., GitHub pull request) on the code host for review and merging.

> NOTE: When pushing the branch Sourcegraph will use a **force push**. Make sure that the branch names are unused, otherwise previous commits will be overwritten.

In the Sourcegraph web UI you'll see a progress indicator for the changesets that are being published and any possible errors:

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/campaigns/publishing_changesets_viewing_progress_and_errors.png" class="screenshot center">

If you run into any errors, you can retry publishing after you've resolved the problem by running `src campaign apply` again.

You don't need to worry about multiple branches or pull requests being created when you retry, because the same branch name will be used and the commit will be overwritten.

## Publishing a subset of changesets

Instead of publishing all changesets at the same time, you can also publish some of a campaign's changesets, by specifying which changesets you want to publish in the `published` field:

```yaml
# ...

changesetTemplate:
  # ...
  published:
    - github.com/sourcegraph/sourcegraph: true
    - github.com/sourcegraph/src-cli: false
```

See [`changesetTemplate.published`](../references/campaign_spec_yaml_reference.md#changesettemplate-published) in the campaign spec reference for more details.

## Publishing changesets as drafts

Some code hosts (GitHub, GitLab) allow publishing changesets as _drafts_. To publish a changeset as a draft, use the `'draft`' value in the `published` field:

```yaml
# ...

changesetTemplate:
  # ...
  published: draft
```

See [`changesetTemplate.published`](../references/campaign_spec_yaml_reference.md#changesettemplate-published) in the campaign spec reference for more details.

## Fully publishing draft changesets

If you have previously published changesets as drafts on code hosts by setting `published` to `draft`, you then fully publish them and take them out of draft mode by updating the `published` to `true`.

See [`changesetTemplate.published`](../references/campaign_spec_yaml_reference.md#changesettemplate-published) in the campaign spec reference for more details.

## Specifying Git commit details

The commit that's created and pushed to the branch uses the details specified in the campaign spec's `changesetTemplate` field.

See [`changesetTemplate.commit`](../references/campaign_spec_yaml_reference.md#changesettemplate-commit) for details on how to set the author and the commit message.
