# Bazel

Sourcegraph is currently migrating to Bazel as its build system and this page is targeted for early adopters which are helping the [#job-fair-bazel](https://sourcegraph.slack.com/archives/C03LUEB7TJS) team to test their work.

## Why do we need a build system?

Building Sourcegraph is a non-trivial task, as it not only ships a frontend and a backend, but also a variety of third parties and components that makes the building process complicated, not only locally but also in CI. Historically, this always have been solved with ad-hoc solutions, such as shell scripts, and caching in various point of the process.

But we're using languages that traditionally don't require their own build systems right? Go and Typescript have their own ecosystem and solve those problems each with their own way right? Yes indeed, but this also means they are never aware of each other and anything linking them requires to be implemented manually, which what we've done so far. Because the way our app is built, as a monolith, it's not trivial to detect things such as the need to rebuild a given Docker image because a change was made in another package, because there is nothing enforcing this structurally in the codebase. So we have to rebuild things, because there is doubt.

On top of that, whenever we attempt at building our app, we also need to fetch many third parties from various locations (GitHub releases, NPM, Go packages, ...). While most of the time, it's working well, any failure in doing so will result in failed build. This may go unnoticed when working locally, but on CI, this can prevent us to build our app for hours at times if we can't fetch the dependency we need, until the problem is resolved upstream. This makes us very dependent on the external world.

In the end, it's not what composes our application that drives us to use a build system, but instead the size it has reached after years of development. We could solve all these problems individually with custom solutions, that would enable us to deterministically say that we need to build X because Y changed. But guess what? The result would pretty much look like a build system. It's a known problem and solutions exists in the wild for us to use.

Finally, build systems provides additional benefits, especially on the security side. Because a build system is by definition aware of every little dependency, we can use that to ensure we react swiftly to CVEs (Common Vulnerabilities and Exposures) produce SBOMs (Software Bill of Materials) for our customers to speed up the upgrade process.

## Why Bazel?

Bazel is the most used build system in the world that is fully language agnostic. It means you can build whatever you want with Bazel, from a tarball containing Markdown files to an iOS app. It's like Make, but much more powerful. Because of its popularity, it means its ecosystem is the biggest and a lot have been written for us to use already.

We could have used others, but that would translate in having to write much more. Building client code for example is a delicate task, because of the complexity of targeting browsers and the pace at which its ecosystem is evolving. So we're avoiding that by using a solution that has been battle tested and proved to still work at scale hundred times bigger than ours and smaller than us.

## What is Bazel?

Bazel sits in between traditional tools that build code, and you, similarly to how Make does one could say. At it's core, Bazel enables you to describe a hierarchy of all the pieces needed to build the application, plus the steps required to build one based on the others.

### What a build system does

Let's take a simple example: we are building a small tool that is written in Go and we ship it with `jq` so our users don't need to install it to use our code. We want our release tarball to contain our app binary, `jq` and our README.

Our codebase would look like this:

```
- README.md
- main.go
- download_jq.sh
```

The result would look like this:

```
- app.tar.gz
  - ./app
  - ./jq
  - README.md
```

To built it we need to perform the following actions:

1. Build `app` with `go build ...`
1. Run `./download_jq.sh` to fetch the `jq` binary
1. Create a tarball containing `app`, `jq` and `README.md`

If we project those actions onto our filetree, it looks like this (let's call it an _action graph_):

```
- app.tar.gz
  - # tar czvf app.tar.gz .
    - ./app
      - # go build main.go -o app
    - ./jq
      - # ./download_jq.sh
    - README.md
```

We can see how we have a tree of _inputs_ forming the final _output_ which is `app.tar.gz`. If all _inputs_ of a given _output_ didn't change, we don't need to build them again right? That's exactly the question that a build system can answer, and more importantly *deterministically*. Bazel is going to store all the checksums of _inputs_ and _outputs_ and will perform only what's required to generate the final _output_.

If our Go code did not change, we're still using the same version of `jq` but the README changed, do I need to generate a new tarball? Yes because the tarball depends on the README as well. If neither changed, we can simply keep the previous tarball. If we do not have Bazel, we need to provide a way to ensure it.

As long as Bazel's cache is warm, we'll never need to run `./download_jq.sh` to download `jq` again, meaning that even if GitHub is down and we can't fetch it, we can still build our tarball.

For Go and Typescript, this means that every dependency, either a Go module or a NPM package will be cached, because Bazel is aware of it. As long as the cache is warm, it will never download it again. We can even tell Bazel to make sure that the checksum of the `jq` binary we're fetching stays the same. If someone were to maliciously swap a `jq` release with a new one, Bazel would catch it, even it was the same exact version.

### Tests are outputs too.

Tests, whether it's a unit test or an integration tests, are _outputs_ when you think about it. Instead of being a file on disk, it's just green or red. So the same exact logic can be applied to them! Do I need to run my unit tests if the code did not change? No you don't, because the _inputs_ for that test did not change.

Let's say you have integration tests driving a browser querying your HTTP API written in Go. A naive way of representing this would be to say that the _inputs_ for that e2e test are the source for the tests. A better version would be to say that the _inputs_ for this tests are also the binary powering your HTTP API. Therefore, changing the Go code would trigger the e2e tests to be ran again, because it's an _input_ and it changed again.

So, building and testing is in the end, practically the same thing.

### Why is Bazel frequently mentioned in a negative light on Reddit|HN|Twitter|... ?

Build systems are solving a complex problem. Assembling a deterministic tree of all the _inputs_ and _outputs_ is not an easy task, especially when your project is becoming less and less trivial. And to enforce it's properties, such as hermeticity and being deterministic, Bazel requires both a "boil the ocean first" approach, where you need to convert almost everything in your project to benefit from it and to learn how to operate it. That's quite the upfront cost and a lot of cognitive weight to absorb, naturally resulting in negative opinions.

In exchange for that, we get a much more robust system, resilient to some unavoidable problems that comes when building your app requires to reach the outside world.

## Bazel for teammates in a hurry

### Bazel vocabulary

- A _rule_ is a function that stitches together parts of the graph.
  - ex: build go code
- A _target_ is a named rule invocation.
  - ex: build the go code for `./app`
  - ex: run the unit tests for `./app`
- A _package_ is a a group of _targets_.
  - ex: we only have one single package in the example above, the root one.

Bazel uses two types of files to define those:

- `WORKSPACE`, which sits at the root of a project and tells Bazel where to find the rules.
  - ex: get the Go _rules_ from that repository on GitHub, in this exact version.
- `BUILD.bazel`, which sits in every folder that contains _targets_.

To reference them, the convention being used is the following: `//pkg1/pkg2:my_target` and you can say things such as `//pkg1/...` to reference all possible _targets_ in a package.

Finally, let's say we have defined in our Bazel project some third party dependencies (a NPM module or a Go package), they will be referenced using the `@` sign.

- `@com_github_keegancsmith_sqlf//:sqlf`

### Sandboxing

Bazel ensures it's running hermetically by sandboxing anything it does. It won't build your code right in your source tree. It will copy all of what's needed to build a particular _target_ in a temporary directory (and nothing more!) and then apply all the rules defined for these _targets_.

This is a *very important* difference from doing things the usual way. If you didn't tell Bazel about an _input_, it won't be built/copied in/over the sandbox. So if your tests are relying testdata for examples, Bazel must be aware of it. This means that it's not possible to change the _outputs_ by accident because you created an additional file in the source tree.

So having to make everything explicit means that the buildfiles (the `BUILD.bazel` files) need to be kept in sync all the time. Luckily, Bazel comes with a solution to automate this process for us.

### Generating buildfiles automatically

Bazel ships with a tool named `Gazelle` whose purpose is to take a look at your source tree and to update the buildfiles for you. Most of the times, it's going to do the right thing. But in some cases, you may have to manually edit the buildfiles to specify what Gazelle cannot guess for you.

Gazelle and Go: It works almost transparently with Go, it will find all your Go code and infer your dependencies from inspecting your imports. Similarly, it will inspect the `go.mod` to lock down the third parties dependencies required. Because of how well Gazelle-go works, it means that most of the time, you can still rely on your normal Go commands to work. But it's recommended to use Bazel because that's what will be used in CI to build the app and ultimately have the final word in saying if yes or no a PR can be merged. See the [cheat sheet section](#bazel-cheat-sheet) for the commands.

Gazelle and the frontend: TODO

### Bazel cheat sheet

#### Keep in mind

- Do not commit file whose name include spaces, Bazel does not like it.
- Do not expect your tests to be executed inside the source tree and to be inside a git repository.
  - They will be executed in the sandbox. Instead create a temp folder and init a git repo manually over there.

#### Building and testing things

- `bazel build [path-to-target]` builds a target.
  - ex `bazel build //lib/...` will build everything under the `/lib/...` folder in the Sourcegraph repository.
- `bazel test [path-to-target]` tests a target.
  - ex `bazel test //lib/...` will run all tests under the `/lib/...` folder in the Sourcegraph repository.
- `bazel run :gazelle` automatically inspect the source tree and update the buildfiles if needed.
- `bazel run //:gazelle-update-repos` automatically inspect the `go.mod` and update the third parties dependencies if needed.

#### Debugging buildfiles

- `bazel query "//[pkg]/..."` See all subpackages of `pkg`.
- `bazel query "//[pkg]:*"` See all targets of `pkg`.
- `bazel query //[pkg] --output location` prints where the buildfile for `pkg` is.
  - ex: `bazel query @com_github_cloudflare_circl//dh/x448 --output location` which allows to inspect the autogenerated buildfile.
- `bazel query "allpaths(pkg1, pkg2)"` list all knowns connections from `pkg1` to `pkg2`
  - ex `bazel query "allpaths(//enterprise/cmd/worker, @go_googleapis//google/api)"`
  - This is very useful when you want to understand what connects a given package to another.

## FAQ

### Go

#### It complains about some missing symbols, but I'm sure they are there since I can see my files

```
ERROR: /Users/tech/work/sourcegraph/internal/redispool/BUILD.bazel:3:11: GoCompilePkg internal/redispool/redispool.a failed: (Exit 1): builder failed: error executing command (from target //internal/redispool:redispool) bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/go_sdk/builder compilepkg -sdk external/go_sdk -installsuffix darwin_arm64 -src internal/redispool/redispool.go -src internal/redispool/sysreq.go ... (remaining 30 arguments skipped)

Use --sandbox_debug to see verbose messages from the sandbox and retain the sandbox build root for debugging
internal/redispool/redispool.go:78:13: undefined: RedisKeyValue
internal/redispool/redispool.go:94:13: undefined: RedisKeyValue
```

OR

```
~/work/sourcegraph U bzl/build-go $ bazel build //dev/sg
INFO: Analyzed target //dev/sg:sg (955 packages loaded, 16719 targets configured).
INFO: Found 1 target...
ERROR: /Users/tech/work/sourcegraph/internal/conf/confdefaults/BUILD.bazel:3:11: GoCompilePkg internal/conf/confdefaults/confdefaults.a failed: (Exit 1): builder failed: error executing command (from target //internal/conf/confdefaults:confdefaults) bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/go_sdk/builder compilepkg -sdk external/go_sdk -installsuffix darwin_arm64 -src internal/conf/confdefaults/confdefaults.go -embedroot '' -embedroot ... (remaining 19 arguments skipped)

Use --sandbox_debug to see verbose messages from the sandbox and retain the sandbox build root for debugging
compilepkg: missing strict dependencies:
	/private/var/tmp/_bazel_tech/3eea80c6015362974b7d423d1f30cb62/sandbox/darwin-sandbox/10/execroot/__main__/internal/conf/confdefaults/confdefaults.go: import of "github.com/russellhaering/gosaml2/uuid"
No dependencies were provided.
Check that imports in Go sources match importpath attributes in deps.
Target //dev/sg:sg failed to build
Use --verbose_failures to see the command lines of failed build steps.
INFO: Elapsed time: 11.559s, Critical Path: 2.93s
INFO: 36 processes: 2 internal, 34 darwin-sandbox.
```


Solution: run `bazel run //:gazelle` to update the buildfiles automatically.


#### My go tests complains about missing testdata

In the case where your testdata lives in `../**`, Gazelle cannot see those on its own, and you need to create a filegroup manually, see https://github.com/sourcegraph/sourcegraph/pull/47605/commits/93c838aad5436dc69f6695cec933bfb84b8ba59a

## Resources

- [Core Bazel (book)](https://www.amazon.com/Core-Bazel-Fast-Builds-People/dp/B08DVDM7BZ):
  - [Bazel User guide](https://bazel.build/docs)
- [Writing a custom rule that depends on an external dep](https://www.youtube.com/watch?v=bhirT014eCE)
- [Patching third parties when they don't build](https://rotemtam.com/2020/10/30/bazel-building-cgo-bindings/)
