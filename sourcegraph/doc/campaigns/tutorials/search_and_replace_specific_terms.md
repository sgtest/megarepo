# Search and replace specific terms

<style>
.markdown-body pre.chroma {
  font-size: 0.75em;
}

img.screenshot {
    max-width: 600px;
    margin: 1em;
    margin-bottom: 0.5em;
    border: 1px solid lightgrey;
    border-radius: 10px;
}

img.center {
  display: block;
  margin: auto
}
</style>

<p class="lead">
Create a campaign that changes wording in every repository.
</p>

### Introduction

This tutorial shows you how to create [a campaign spec](../explanations/introduction_to_campaigns.md#campaign-spec) that replaces the words `whitelist` and `blacklist` with `allowlist` and `denylist` in every Markdown file across your entire code base.

The campaign spec can be easily changed to search and replace other terms in other file types.

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/campaigns/tutorials/use_allowlist_denylist_wording_teaser.png" class="screenshot center">

### Prerequisites

We recommend using the latest version of Sourcegraph when working with campaigns and that you have a basic understanding of how to create campaign specs and run them. See the following documents for more information:

1. ["Quickstart"](../quickstart.md)
1. ["Introduction to campaigns"](../explanations/introduction_to_campaigns.md)

### Create the campaign spec

Save the following campaign spec YAML as `allowlist-denylist.campaign.yaml`:

```yaml
name: use-allowlist-denylist-wording
description: This campaign updates our Markdown docs to use the terms "allowlist" and "denylist" instead of "whitelist" and "blacklist".

# Search for repositories in which the term "whitelist" or "blacklist" appears
# in Markdown files.
on:
  - repositoriesMatchingQuery: whitelist OR blacklist lang:markdown -file:vendor -file:node_modules

# In each repository
steps:
  # find all *.md or *.markdown files, that are not in a vendor or node_modules
  # folder, and replace the terms in them:
  - run: |
      find . -type f \( -name '*.md' -or -name '*.markdown' \) -not -path "*/vendor/*" -not -path "*/node_modules/*" |\
      xargs sed -i 's/whitelist/allowlist/g; s/blacklist/denylist/g'
    container: alpine:3

# Describe the changeset (e.g., GitHub pull request) you want for each repository.
changesetTemplate:
  title: Replace whitelist/blacklist with allowlist/denylist
  body: This replaces the terms whitelist/blacklist in Markdown files with allowlist/denylist
  branch: campaigns/allowlist-denylist # Push the commit to this branch.
  commit:
    message: Replace whitelist/blacklist with allowlist/denylist
  published: false
```

### Create the campaign

1. In your terminal, run this command:

    <pre>src campaign preview -f use-allowlist-denylist-wording.campaign.yaml</pre>
1. Wait for it to run and compute the changes for each repository.
    <img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/campaigns/tutorials/use_allowlist_denylist_wording_wait_run.png" class="screenshot">
1. Open the preview URL that the command printed out.
    <img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/campaigns/tutorials/use_allowlist_denylist_wording_click_url.png" class="screenshot">
1. Examine the preview. Confirm that the changes are what you intended. If not, edit your campaign spec and then rerun the command above.
    <img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/campaigns/tutorials/use_allowlist_denylist_wording_preview.png" class="screenshot">
1. Click the **Apply spec** button to create the campaign.
1. Feel free to then publish the changesets (i.e. create pull requests and merge requests) by [modifying the `published` attribute in the campaign spec](../references/campaign_spec_yaml_reference.md#changesettemplate-published) and re-running the `src campaign preview` command.

### Using `ruplacer` to replace terms in multiple case styles

With [ruplacer](https://github.com/TankerHQ/ruplacer) we can easily search and replace terms in multiple case styles: `white_list`, `WhiteList`, `WHITE_LIST` etc.

The easiest way to use `ruplacer` in our campaign spec would look like this:

```yaml
steps:
    # Install and use ruplacer to replace words in case-style variations
  - run: |
      cargo install ruplacer \
      && find . -type f \( -name '*.md' -or -name '*.markdown' \) -not -path "*/vendor/*" -not -path "*/node_modules/*" >> /tmp/find_result.txt \
      && cat /tmp/find_result.txt | while read file;
      do
        ruplacer --subvert whitelist allowlist --go ${file} || echo "nothing to replace";
        ruplacer --subvert blacklist denylist --go ${file} || echo "nothing to replace";
      done
    # Use the rust image in our container
    container: rust
```

But there's a problem with that approach: every new execution of `src campaign preview` has to execute the `cargo install ruplacer` command again. And if you're tweaking which terms you're replacing, that performance cost can become too much quite fast.

A better option would be to to build a small Docker image in which `ruplacer` is already installed.

To do that, save the following in a `Dockerfile`:

```dockerfile
FROM rust
RUN cargo install ruplacer
```

Then build a Docker image out of it, tagged with `ruplacer`, by running the following command in your terminal:

```
docker build . -t ruplacer
```

Once that is done, we can use the following `steps` in our campaign spec:

```yaml
steps:
  - run: |
      find . -type f \( -name '*.md' -or -name '*.markdown' \) -not -path "*/vendor/*" -not -path "*/node_modules/*" >> /tmp/find_result.txt \
      && cat /tmp/find_result.txt | while read file;
      do
        ruplacer --subvert whitelist allowlist --go ${file} || echo "nothing to replace";
        ruplacer --subvert blacklist denylist --go ${file} || echo "nothing to replace";
      done
    # Use the newly-built ruplacer image
    container: ruplacer
```

Save the file and run the `src campaign preview` command from above again to use `ruplacer` to replace variations of terms.
