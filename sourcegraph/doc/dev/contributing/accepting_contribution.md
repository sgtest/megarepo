# How to accept an external contribution

This page outlines how to accept a contribution to the [Sourcegraph repository](https://github.com/sourcegraph/sourcegraph) from someone outside the Sourcegraph team.

## CLA-bot

1. Check if a contributor signed the CLA [here](https://docs.google.com/spreadsheets/d/1_iBZh9PJi-05vTnlQ3GVeeRe8H3Wq1_FZ49aYrsHGLQ/edit?usp=sharing). All fields should be filled with valid data to proceed with the pull request.
2. If the CLA is signed — update the CLA-bot configuration [here](https://github.com/sourcegraph/clabot-config/edit/main/.clabot) by adding a contributor name to the `contributors` field, preserving the alphabetical order.
3. Comment on the pull request: `@cla-bot check`.
4. The `verification/cla-signed` workflow should become green. 🎉

## Buildkite

To request a Buildkite build for a pull request from a fork, check out the branch and use [the `sg` CLI](../background-information/sg/index.md) to request a build after reviewing the code:

```sh
sg ci build
```

To check out a pull request's code locally, use [the `gh` CLI](https://cli.github.com/):

```sh
gh pr checkout $NUMBER
```

Alternatively, it is also possible to check out the branch without having to re-clone the forked repo by running:

```sh
git fetch git@github.com:$THEIR_USERNAME/sourcegraph $THEIR_BRANCH:$THEIR_BRANCH
```

Make sure that the created branch name exactly matches their branch name, otherwise Buildkite will not match the created build to their branch.
