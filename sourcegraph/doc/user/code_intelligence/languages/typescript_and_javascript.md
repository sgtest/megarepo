# TypeScript and JavaScript

This guide is meant to provide specific instructions to get you producing index data in LSIF as quickly as possible. The [LSIF quick start](../lsif_quickstart.md) and [CI configuration](../adding_lsif_to_workflows.md) guides provide more in depth descriptions of each step and a lot of helpful context that we haven't duplicated in each language guide.

## Manual indexing

1. Install [lsif-node](https://github.com/sourcegraph/lsif-node) with `npm install -g @sourcegraph/lsif-tsc` or your favorite method of installing npm packages.

1. Install the [Sourcegraph CLI](https://github.com/sourcegraph/src-cli) with
   ```
   curl -L https://sourcegraph.com/.api/src-cli/src_linux_amd64 -o /usr/local/bin/src
   chmod +x /usr/local/bin/src
   ```
   - **macOS**: replace `linux` with `darwin` in the URL
   - **Windows**: visit [the CLI's repo](https://github.com/sourcegraph/src-cli) for further instructions

1. `cd` into your project's root (where the package.json/tsconfig.json) and run the following:
   ```
   # for typescript projects
   lsif-tsc -p .
   # for javascript projects
   lsif-tsc **/*.js --allowJs --checkJs
   ```
   Check out the tool's documentation if you're having trouble getting `lsif-tsc` to work. It accepts any options `tsc` does, so it shouldn't be too hard to get it running on your project.

1. Upload the data to a Sourcegraph instance with
   ```
   # for private instances
   src -endpoint=<your sourcegraph endpoint> lsif upload
   # for public instances
   src lsif upload -github-token=<your github token>
   ```
   Visit the [LSIF quickstart](../lsif_quickstart.md) for more information about the upload command.

The upload command will provide a URL you can visit to see the upload's status, and when it's done you can visit the repo and check out the difference in code navigation quality! To troubleshoot issues, visit the more in depth [LSIF quickstart](../lsif_quickstart.md) guide and check out the documentation for the `lsif-node` and `src-cli` tools.

## Automated indexing

We provide the docker images `sourcegraph/lsif-node` and `sourcegraph/src-cli` to make automating this process in your favorite CI framework as easy as possible. Note that the `lsif-node` image bundles `src-cli` so the second image may not be necessary.

Here's some examples in a couple popular frameworks, just substitute the indexer and upload commands with what works for your project locally:

### GitHub Actions
```yaml
jobs:
  lsif-node:
    runs-on: ubuntu-latest
    container: sourcegraph/lsif-node:latest
    steps:
      - uses: actions/checkout@v1
      - name: Install dependencies
        run: npm install
      - name: Generate LSIF data
        run: lsif-tsc -p .
      - name: Upload LSIF data
        run: src lsif upload -github-token=${{ secrets.GITHUB_TOKEN }}
```
Note that if you need to install your dependencies in a custom container, you can use our containers as github actions. Try these steps instead:
```yaml
jobs:
  lsif-node:
    runs-on: ubuntu-latest
    container: my-awesome-container
    steps:
      - uses: actions/checkout@v1
      - name: Install dependencies
        run: <install dependencies>
      - name: Generate LSIF data
        uses: sourcegraph/lsif-node:latest
        with:
          args: lsif-tsc -p .
      - name: Upload LSIF data
        uses: sourcegraph/src-cli:latest
        with:
          args: src lsif upload -github-token=${{ secrets.GITHUB_TOKEN }}
```

### CircleCI
```yaml
jobs:
  lsif-node:
    docker:
      - image: sourcegraph/lsif-node:latest
    steps:
      - checkout
      - run: npm install
      - run: lsif-tsc -p .
      - run: src lsif upload -github-token=<<parameters.github-token>>

workflows:
  lsif-node:
    jobs:
      - lsif-node
```
Note that if you need to install your dependencies in a custom container, may need to use CircleCI's caching features to share the build environment with our container. It may alternately be easier to add our tools to your container, but here's an example using caches:
```yaml
jobs:
  install-deps:
    docker:
      - image: my-awesome-container
    steps:
      - checkout
      - <install dependencies>
      - save_cache:
          paths:
            - node_modules
          key: dependencies

jobs:
  lsif-node:
    docker:
      - image: sourcegraph/lsif-node:latest
    steps:
      - checkout
      - restore_cache:
          keys:
            - dependencies
      - run: lsif-tsc -p .
      - run: src lsif upload -github-token=<<parameters.github-token>>

workflows:
  lsif-node:
    jobs:
      - install-deps
      - lsif-node:
          requires:
            - install-deps
```

# Travis CI
```yaml
services:
  - docker

jobs:
  include:
    - stage: lsif-node
      script:
      - |
        docker run --rm -v $(pwd):/src -w /src sourcegraph/lsif-node:latest /bin/sh -c \
          "lsif-tsc -p .; src lsif upload -github-token=$GITHUB_TOKEN"
```
