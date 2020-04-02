# Using Perforce repositories with Sourcegraph

You can use [Perforce](https://perforce.com) repositories with Sourcegraph by using the [git p4](https://git-scm.com/docs/git-p4) adapter, which creates an equivalent Git repository from a Perforce repository, and [`src-expose`](../external_service/non-git.md), Sourcegraph's tool for serving local directories.

Screenshot of using Sourcegraph for code navigation in a Perforce repository:

![Viewing a Perforce repository on Sourcegraph](https://storage.googleapis.com/sourcegraph-assets/git-p4-example.png)

## Instructions

### Prerequisites

- Git
- Perforce `p4` CLI configured to access your Perforce repository
- `git p4` (see "[Adding `git p4` to an existing install](https://git.wiki.kernel.org/index.php/GitP4#Adding_git-p4_to_an_existing_install)")
- [`src-expose`](../external_service/non-git.md)

### Create an equivalent Git repository and serve it to Sourcegraph

For each Perforce repository you want to use with Sourcegraph, follow these steps:

1. Create a local Git repository with the contents of your Perforce repository: `git p4 clone //DEPOT/PATH@all` (replace `//DEPOT/PATH` with the Perforce repository path).
1. `cd PATH` to enter the directory of the new local Git repository.
1. Run `src-expose serve dir1 dir2 dir3`, replacing the directories here with the directories that contain the new local Git repository.
1. Follow the instructions in the [`src-expose` Quickstart](../external_service/non-git.md#quickstart) to add the repositories to your Sourcegraph instance.

### Updating Perforce repositories

To update the repository after new Perforce commits are made, run `git p4 sync` in the local repository directory. These changes will be automatically reflected in Sourcegraph as long as `src-expose serve` is running.

We recommend running this command on a periodic basis using a cron job, or some other scheduler. The frequency will dictate how fresh the code is in Sourcegraph, and can range from once every 10s to once per day, depending on how large your codebase is and how long it takes `git p4 sync` to complete.

### Alternative to `src-expose`: push the new Git repository to a code host

If you prefer, you can skip using `src-expose`, and instead push the new local Git repository to a Git-based code host of your choice. For updates, you would run `git p4 sync && git push` periodically.

If you do this, the repositories you created on your Git host are normal Git repositories, so you can [add the repositories to Sourcegraph](index.md) as you would any other Git repositories.

### Alternative for extra-large codebases

The instructions below will help you get Perforce repositories on Sourcegraph quickly and easily, while retaining all code change history. If your Perforce codebase is large enough that converting it to Git takes long enough to cause noticeable staleness on Sourcegraph, you can use `src-expose`'s [optional syncing functionality](../external_service/non-git.md#syncing-repositories) along with a faster fetching command (like `p4 sync` instead of `git p4 sync`) to periodically fetch and squash changes without trying to preserve the original Perforce history.

## Known issues

We intend to improve Sourcegraph's Perforce support in the future. Please [file an issue](https://github.com/sourcegraph/sourcegraph/issues) to help us prioritize any specific improvements you'd like to see.

- Sourcegraph was initially built for Git repositories only, so it exposes Git concepts that are meaningless for converted Perforce repositories, such as the commit SHA, branches, and tags.
- The commit messages for a Perforce repository converted to a Git repository have an extra line at the end with Perforce information, such as `[git-p4: depot-paths = "//guest/acme_org/myproject/": change = 12345]`.
