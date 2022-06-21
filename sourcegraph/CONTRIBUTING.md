# Contributing

## What contributions are accepted?

In short, we are open to nearly all contributions! We love feedback in all forms, issues, comments, PRs, etc!

## Contributing to `enterprise/` code

Anyone is free to contribute changes to any file in this repository, including `enterprise/` code. Once you open a pull request, the CLA bot will require you sign our contributor CLA before we can accept the change.

## Contributing large changes, new features, etc.

Unless you feel confident your change will be accepted (trivial bug fixes, code cleanup, etc) you should first create an issue or a [Sourcegraph RFC](https://handbook.sourcegraph.com/communication/rfcs#external-contributors) (preferred for bigger changes) to discuss your change with us. This lets us all discuss the design and proposed implementation of your change, which helps ensure your time is well spent and that your contribution will be accepted.

> Exception: If you contribute functionality that already exists as a [paid Sourcegraph feature](https://about.sourcegraph.com/pricing/), we are unlikely to accept it. Consult us beforehand for a definitive answer. (We'll add more details about the process here, and they'll be similar to [GitLab's stewardship principles](https://about.gitlab.com/stewardship/#contributing-an-existing-ee-feature-to-ce).)

## Code of Conduct

All interactions with the Sourcegraph open source project are governed by the
[Sourcegraph Community Code of Conduct](https://handbook.sourcegraph.com/company-info-and-process/community/code_of_conduct/).

## How to contribute

1. Select one of the issues labeled as [good first issue](https://github.com/orgs/sourcegraph/projects/210).
2. Clone the repo: `git clone https://github.com/sourcegraph/sourcegraph`.
3. [Set up your development environment](https://docs.sourcegraph.com/dev/contributing) to run the project locally.
4. Before creating a pull request, ensure that [recommended checks](https://docs.sourcegraph.com/dev/contributing) pass locally. We're actively working on making our CI pipeline public to automate this step.
5. **IMPORTANT:** Once you have a pull request ready to review, the 'verification/cla-signed' check will be flagged, and you will be prompted to sign the CLA with a link provided by our bot. Once you sign, add a comment tagging `@sourcegraph/contribution-reviewers`. After that your pull request will be ready for review.
   - For Sourcegraph team members, see [these notes](https://docs.sourcegraph.com/dev/contributing/accepting_contribution#cla-bot) for how to verify that the CLA has been signed.
6. Once you've chosen an issue, **comment on it to announce that you will be working on it**, making it visible for others that this issue is being tackled. If you end up not creating a pull request for this issue, please delete your comment.
7. If you have any questions, please [refer to the docs first](https://docs.sourcegraph.com/). If you don’t find any relevant information, mention the issue author.
8. The issue author will try to provide guidance. Sourcegraph always works in async mode. We will try to answer as soon as possible, but please keep time zones differences in mind.

## Can I pick up this issue?

All open issues are not yet solved. If the task is interesting to you, take it and feel free to do it. There is no need to ask for permission or get in line. Even if someone else can do the task faster than you, don't stop - your solution may be better. It is the beauty of Open Source!

## Relevant development docs

### Getting applications up and running

- [Getting Started Guide](https://docs.sourcegraph.com/dev/getting-started)
- [Troubleshooting section](https://docs.sourcegraph.com/dev/how-to/troubleshooting_local_development)

### How to write tests

- [How to write tests](https://docs.sourcegraph.com/dev/how-to/testing)
- [Testing principles](https://docs.sourcegraph.com/dev/background-information/testing_principles)
- [Testing web code](https://docs.sourcegraph.com/dev/background-information/testing_web_code)

### Pull Requests

- [How to structure](https://docs.sourcegraph.com/dev/background-information/pull_request_reviews#what-makes-an-effective-pull-request-pr)
- [Size guidelines](https://handbook.sourcegraph.com/engineering/enablement#prefer-small-prs-400-lines)
- Git branch name convention: `[developer-initials]/short-feature-description`
- [Examples on Github](https://github.com/sourcegraph/sourcegraph/pulls?q=is%3Apr+label%3Ateam%2Ffrontend-platform)
- (For Sourcegraph team) [How to accept contributions](https://docs.sourcegraph.com/dev/contributing/accepting_contribution)
