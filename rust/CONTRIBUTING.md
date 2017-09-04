# Contributing to Rust

Thank you for your interest in contributing to Rust! There are many ways to
contribute, and we appreciate all of them. This document is a bit long, so here's
links to the major sections:

* [Feature Requests](#feature-requests)
* [Bug Reports](#bug-reports)
* [The Build System](#the-build-system)
* [Pull Requests](#pull-requests)
* [Writing Documentation](#writing-documentation)
* [Issue Triage](#issue-triage)
* [Out-of-tree Contributions](#out-of-tree-contributions)
* [Helpful Links and Information](#helpful-links-and-information)

If you have questions, please make a post on [internals.rust-lang.org][internals] or
hop on [#rust-internals][pound-rust-internals].

As a reminder, all contributors are expected to follow our [Code of Conduct][coc].

[pound-rust-internals]: http://chat.mibbit.com/?server=irc.mozilla.org&channel=%23rust-internals
[internals]: https://internals.rust-lang.org
[coc]: https://www.rust-lang.org/conduct.html

## Feature Requests

To request a change to the way that the Rust language works, please open an
issue in the [RFCs repository](https://github.com/rust-lang/rfcs/issues/new)
rather than this one. New features and other significant language changes
must go through the RFC process.

## Bug Reports

While bugs are unfortunate, they're a reality in software. We can't fix what we
don't know about, so please report liberally. If you're not sure if something
is a bug or not, feel free to file a bug anyway.

**If you believe reporting your bug publicly represents a security risk to Rust users,
please follow our [instructions for reporting security vulnerabilities](https://www.rust-lang.org/security.html)**.

If you have the chance, before reporting a bug, please [search existing
issues](https://github.com/rust-lang/rust/search?q=&type=Issues&utf8=%E2%9C%93),
as it's possible that someone else has already reported your error. This doesn't
always work, and sometimes it's hard to know what to search for, so consider this
extra credit. We won't mind if you accidentally file a duplicate report.

Opening an issue is as easy as following [this
link](https://github.com/rust-lang/rust/issues/new) and filling out the fields.
Here's a template that you can use to file a bug, though it's not necessary to
use it exactly:

    <short summary of the bug>

    I tried this code:

    <code sample that causes the bug>

    I expected to see this happen: <explanation>

    Instead, this happened: <explanation>

    ## Meta

    `rustc --version --verbose`:

    Backtrace:

All three components are important: what you did, what you expected, what
happened instead. Please include the output of `rustc --version --verbose`,
which includes important information about what platform you're on, what
version of Rust you're using, etc.

Sometimes, a backtrace is helpful, and so including that is nice. To get
a backtrace, set the `RUST_BACKTRACE` environment variable to a value
other than `0`. The easiest way
to do this is to invoke `rustc` like this:

```bash
$ RUST_BACKTRACE=1 rustc ...
```

## The Build System

Rust's build system allows you to bootstrap the compiler, run tests &
benchmarks, generate documentation, install a fresh build of Rust, and more.
It's your best friend when working on Rust, allowing you to compile & test
your contributions before submission.

The build system lives in [the `src/bootstrap` directory][bootstrap] in the
project root. Our build system is itself written in Rust and is based on Cargo
to actually build all the compiler's crates. If you have questions on the build
system internals, try asking in [`#rust-internals`][pound-rust-internals].

[bootstrap]: https://github.com/rust-lang/rust/tree/master/src/bootstrap/

### Configuration

Before you can start building the compiler you need to configure the build for
your system. In most cases, that will just mean using the defaults provided
for Rust.

To change configuration, you must copy the file `config.toml.example`
to `config.toml` in the directory from which you will be running the build, and
change the settings provided.

There are large number of options provided in this config file that will alter the
configuration used in the build process. Some options to note:

#### `[llvm]`:
- `ccache = true` - Use ccache when building llvm

#### `[build]`:
- `compiler-docs = true` - Build compiler documentation

#### `[rust]`:
- `debuginfo = true` - Build a compiler with debuginfo
- `optimize = false` - Disable optimizations to speed up compilation of stage1 rust

For more options, the `config.toml` file contains commented out defaults, with
descriptions of what each option will do.

Note: Previously the `./configure` script was used to configure this
project. It can still be used, but it's recommended to use a `config.toml`
file. If you still have a `config.mk` file in your directory - from
`./configure` - you may need to delete it for `config.toml` to work.

### Building

The build system uses the `x.py` script to control the build process. This script
is used to build, test, and document various parts of the compiler. You can
execute it as:

```sh
python x.py build
```

On some systems you can also use the shorter version:

```sh
./x.py build
```

To learn more about the driver and top-level targets, you can execute:

```sh
python x.py --help
```

The general format for the driver script is:

```sh
python x.py <command> [<directory>]
```

Some example commands are `build`, `test`, and `doc`. These will build, test,
and document the specified directory. The second argument, `<directory>`, is
optional and defaults to working over the entire compiler. If specified,
however, only that specific directory will be built. For example:

```sh
# build the entire compiler
python x.py build

# build all documentation
python x.py doc

# run all test suites
python x.py test

# build only the standard library
python x.py build src/libstd

# test only one particular test suite
python x.py test src/test/rustdoc

# build only the stage0 libcore library
python x.py build src/libcore --stage 0
```

You can explore the build system through the various `--help` pages for each
subcommand. For example to learn more about a command you can run:

```
python x.py build --help
```

To learn about all possible rules you can execute, run:

```
python x.py build --help --verbose
```

Note: Previously `./configure` and `make` were used to build this project.
They are still available, but `x.py` is the recommended build system.

### Useful commands

Some common invocations of `x.py` are:

- `x.py build --help` - show the help message and explain the subcommand
- `x.py build src/libtest --stage 1` - build up to (and including) the first
  stage. For most cases we don't need to build the stage2 compiler, so we can
  save time by not building it. The stage1 compiler is a fully functioning
  compiler and (probably) will be enough to determine if your change works as
  expected.
- `x.py build src/rustc --stage 1` - This will build just rustc, without libstd.
  This is the fastest way to recompile after you changed only rustc source code.
  Note however that the resulting rustc binary won't have a stdlib to link
  against by default. You can build libstd once with `x.py build src/libstd`,
  but it is only guaranteed to work if recompiled, so if there are any issues
  recompile it.
- `x.py test` - build the full compiler & run all tests (takes a while). This
  is what gets run by the continuous integration system against your pull
  request. You should run this before submitting to make sure your tests pass
  & everything builds in the correct manner.
- `x.py test src/libstd --stage 1` - test the standard library without
  recompiling stage 2.
- `x.py test src/test/run-pass --test-args TESTNAME` - Run a matching set of
  tests.
  - `TESTNAME` should be a substring of the tests to match against e.g. it could
    be the fully qualified test name, or just a part of it.
    `TESTNAME=collections::hash::map::test_map::test_capacity_not_less_than_len`
    or `TESTNAME=test_capacity_not_less_than_len`.
- `x.py test src/test/run-pass --stage 1 --test-args <substring-of-test-name>` -
  Run a single rpass test with the stage1 compiler (this will be quicker than
  running the command above as we only build the stage1 compiler, not the entire
  thing).  You can also leave off the directory argument to run all stage1 test
  types.
- `x.py test src/libcore --stage 1` - Run stage1 tests in `libcore`.
- `x.py test src/tools/tidy` - Check that the source code is in compliance with
  Rust's style guidelines. There is no official document describing Rust's full
  guidelines as of yet, but basic rules like 4 spaces for indentation and no
  more than 99 characters in a single line should be kept in mind when writing
  code.

### Using your local build

If you use Rustup to manage your rust install, it has a feature called ["custom
toolchains"][toolchain-link] that you can use to access your newly-built compiler
without having to install it to your system or user PATH. If you've run `python
x.py build`, then you can add your custom rustc to a new toolchain like this:

[toolchain-link]: https://github.com/rust-lang-nursery/rustup.rs#working-with-custom-toolchains-and-local-builds

```
rustup toolchain link <name> build/<host-triple>/stage2
```

Where `<host-triple>` is the build triple for the host (the triple of your
computer, by default), and `<name>` is the name for your custom toolchain. (If you
added `--stage 1` to your build command, the compiler will be in the `stage1`
folder instead.) You'll only need to do this once - it will automatically point
to the latest build you've done.

Once this is set up, you can use your custom toolchain just like any other. For
example, if you've named your toolchain `local`, running `cargo +local build` will
compile a project with your custom rustc, setting `rustup override set local` will
override the toolchain for your current directory, and `cargo +local doc` will use
your custom rustc and rustdoc to generate docs. (If you do this with a `--stage 1`
build, you'll need to build rustdoc specially, since it's not normally built in
stage 1. `python x.py build --stage 1 src/libstd src/tools/rustdoc` will build
rustdoc and libstd, which will allow rustdoc to be run with that toolchain.)

## Pull Requests

Pull requests are the primary mechanism we use to change Rust. GitHub itself
has some [great documentation][pull-requests] on using the Pull Request feature.
We use the "fork and pull" model [described here][development-models], where
contributors push changes to their personal fork and create pull requests to
bring those changes into the source repository.

[pull-requests]: https://help.github.com/articles/about-pull-requests/
[development-models]: https://help.github.com/articles/about-collaborative-development-models/

Please make pull requests against the `master` branch.

Compiling all of `./x.py test` can take a while. When testing your pull request,
consider using one of the more specialized `./x.py` targets to cut down on the
amount of time you have to wait. You need to have built the compiler at least
once before running these will work, but that’s only one full build rather than
one each time.

    $ python x.py test --stage 1

is one such example, which builds just `rustc`, and then runs the tests. If
you’re adding something to the standard library, try

    $ python x.py test src/libstd --stage 1

Please make sure your pull request is in compliance with Rust's style
guidelines by running

    $ python x.py test src/tools/tidy

Make this check before every pull request (and every new commit in a pull
request) ; you can add [git hooks](https://git-scm.com/book/en/v2/Customizing-Git-Git-Hooks)
before every push to make sure you never forget to make this check.

All pull requests are reviewed by another person. We have a bot,
@rust-highfive, that will automatically assign a random person to review your
request.

If you want to request that a specific person reviews your pull request,
you can add an `r?` to the message. For example, Steve usually reviews
documentation changes. So if you were to make a documentation change, add

    r? @steveklabnik

to the end of the message, and @rust-highfive will assign @steveklabnik instead
of a random person. This is entirely optional.

After someone has reviewed your pull request, they will leave an annotation
on the pull request with an `r+`. It will look something like this:

    @bors: r+ 38fe8d2

This tells @bors, our lovable integration bot, that your pull request has
been approved. The PR then enters the [merge queue][merge-queue], where @bors
will run all the tests on every platform we support. If it all works out,
@bors will merge your code into `master` and close the pull request.

[merge-queue]: https://buildbot2.rust-lang.org/homu/queue/rust

Speaking of tests, Rust has a comprehensive test suite. More information about
it can be found
[here](https://github.com/rust-lang/rust-wiki-backup/blob/master/Note-testsuite.md).

### External Dependencies

Currently building Rust will also build the following external projects:

* [clippy](https://github.com/rust-lang-nursery/rust-clippy)

If your changes break one of these projects, you need to fix them by opening
a pull request against the broken project. When you have opened a pull request,
you can point the submodule at your pull request by calling

```
git fetch origin pull/$id_of_your_pr/head:my_pr
git checkout my_pr
```

within the submodule's directory. Don't forget to also add your changes with

```
git add path/to/submodule
```

outside the submodule.

It can also be more convenient during development to set `submodules = false`
in the `config.toml` to prevent `x.py` from resetting to the original branch.

## Writing Documentation

Documentation improvements are very welcome. The source of `doc.rust-lang.org`
is located in `src/doc` in the tree, and standard API documentation is generated
from the source code itself.

Documentation pull requests function in the same way as other pull requests,
though you may see a slightly different form of `r+`:

    @bors: r+ 38fe8d2 rollup

That additional `rollup` tells @bors that this change is eligible for a 'rollup'.
To save @bors some work, and to get small changes through more quickly, when
@bors attempts to merge a commit that's rollup-eligible, it will also merge
the other rollup-eligible patches too, and they'll get tested and merged at
the same time.

To find documentation-related issues, sort by the [T-doc label][tdoc].

[tdoc]: https://github.com/rust-lang/rust/issues?q=is%3Aopen%20is%3Aissue%20label%3AT-doc

You can find documentation style guidelines in [RFC 1574][rfc1574].

[rfc1574]: https://github.com/rust-lang/rfcs/blob/master/text/1574-more-api-documentation-conventions.md#appendix-a-full-conventions-text

In many cases, you don't need a full `./x.py doc`. You can use `rustdoc` directly
to check small fixes. For example, `rustdoc src/doc/reference.md` will render
reference to `doc/reference.html`. The CSS might be messed up, but you can
verify that the HTML is right.

## Issue Triage

Sometimes, an issue will stay open, even though the bug has been fixed. And
sometimes, the original bug may go stale because something has changed in the
meantime.

It can be helpful to go through older bug reports and make sure that they are
still valid. Load up an older issue, double check that it's still true, and
leave a comment letting us know if it is or is not. The [least recently
updated sort][lru] is good for finding issues like this.

Contributors with sufficient permissions on the Rust repo can help by adding
labels to triage issues:

* Yellow, **A**-prefixed labels state which **area** of the project an issue
  relates to.

* Magenta, **B**-prefixed labels identify bugs which are **blockers**.

* Green, **E**-prefixed labels explain the level of **experience** necessary
  to fix the issue.

* Red, **I**-prefixed labels indicate the **importance** of the issue. The
  [I-nominated][inom] label indicates that an issue has been nominated for
  prioritizing at the next triage meeting.

* Orange, **P**-prefixed labels indicate a bug's **priority**. These labels
  are only assigned during triage meetings, and replace the [I-nominated][inom]
  label.

* Blue, **T**-prefixed bugs denote which **team** the issue belongs to.

* Dark blue, **beta-** labels track changes which need to be backported into
  the beta branches.

* The purple **metabug** label marks lists of bugs collected by other
  categories.

If you're looking for somewhere to start, check out the [E-easy][eeasy] tag.

[inom]: https://github.com/rust-lang/rust/issues?q=is%3Aopen+is%3Aissue+label%3AI-nominated
[eeasy]: https://github.com/rust-lang/rust/issues?q=is%3Aopen+is%3Aissue+label%3AE-easy
[lru]: https://github.com/rust-lang/rust/issues?q=is%3Aissue+is%3Aopen+sort%3Aupdated-asc

## Out-of-tree Contributions

There are a number of other ways to contribute to Rust that don't deal with
this repository.

Answer questions in [#rust][pound-rust], or on [users.rust-lang.org][users],
or on [StackOverflow][so].

Participate in the [RFC process](https://github.com/rust-lang/rfcs).

Find a [requested community library][community-library], build it, and publish
it to [Crates.io](http://crates.io). Easier said than done, but very, very
valuable!

[pound-rust]: http://chat.mibbit.com/?server=irc.mozilla.org&channel=%23rust
[users]: https://users.rust-lang.org/
[so]: http://stackoverflow.com/questions/tagged/rust
[community-library]: https://github.com/rust-lang/rfcs/labels/A-community-library

## Helpful Links and Information

For people new to Rust, and just starting to contribute, or even for
more seasoned developers, some useful places to look for information
are:

* The [Rust Internals forum][rif], a place to ask questions and
  discuss Rust's internals
* The [generated documentation for rust's compiler][gdfrustc]
* The [rust reference][rr], even though it doesn't specifically talk about Rust's internals, it's a great resource nonetheless
* Although out of date, [Tom Lee's great blog article][tlgba] is very helpful
* [rustaceans.org][ro] is helpful, but mostly dedicated to IRC
* The [Rust Compiler Testing Docs][rctd]
* For @bors, [this cheat sheet][cheatsheet] is helpful (Remember to replace `@homu` with `@bors` in the commands that you use.)
* **Google!** ([search only in Rust Documentation][gsearchdocs] to find types, traits, etc. quickly)
* Don't be afraid to ask! The Rust community is friendly and helpful.

[gdfrustc]: http://manishearth.github.io/rust-internals-docs/rustc/
[gsearchdocs]: https://www.google.com/search?q=site:doc.rust-lang.org+your+query+here
[rif]: http://internals.rust-lang.org
[rr]: https://doc.rust-lang.org/book/README.html
[tlgba]: http://tomlee.co/2014/04/a-more-detailed-tour-of-the-rust-compiler/
[ro]: http://www.rustaceans.org/
[rctd]: ./src/test/COMPILER_TESTS.md
[cheatsheet]: https://buildbot2.rust-lang.org/homu/
