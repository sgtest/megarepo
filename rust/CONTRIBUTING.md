## Pull request procedure

Pull requests should be targeted at Rust's `incoming` branch (note that by default Github will aim them at the `master` branch) -- see "Changing The Commit Range and Destination Repository" in Github's documentation on [pull requests](https://help.github.com/articles/using-pull-requests). Before pushing to your Github repo and issuing the pull request, please do two things:

1. [Rebase](http://git-scm.com/book/en/Git-Branching-Rebasing) your local changes against the `incoming` branch. Resolve any conflicts that arise.
2. Run the full Rust test suite with the `make check` command. You're not off the hook even if you just stick to documentation; code examples in the docs are tested as well!

Pull requests will be treated as "review requests", and we will give feedback we expect to see corrected on [style](https://github.com/mozilla/rust/wiki/Note-style-guide) and substance before pulling. Changes contributed via pull request should focus on a single issue at a time, like any other. We will not look kindly on pull-requests that try to "sneak" unrelated changes in.

Normally, all pull requests must include regression tests (see [[Note-testsuite]]) that test your change. Occasionally, a change will be very difficult to test for. In those cases, please include a note in your commit message explaining why.

For more details, please refer to [[Note-development-policy]].