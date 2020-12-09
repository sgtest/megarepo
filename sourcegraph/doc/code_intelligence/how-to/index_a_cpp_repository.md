# Indexing a C++ repository with LSIF

This guide walks through setting up LSIF generation for a C++ codebase using
[`lsif-clang`](https://github.com/sourcegraph/lsif-clang). These instructions should apply to any
C++ project that is buildable with `clang` or `clang++`. (This should also cover most projects built
with `gcc` or `g++`.)

## Local dev setup

### With Docker (recommended)

1. Copy the files in the [`lsif-docker` directory of
   sourcegraph/tesseract](https://github.com/sourcegraph/tesseract/tree/master/lsif-docker) to a
   local `lsif-docker` directory in your C++ repository (the one you wish to index).

1. Replace the contents of `lsif-docker/install_build_deps.sh` with commands that install any
   requisite build dependencies of the project. These should be dependencies that **do not** vary
   from revision to revision.

1. Modify `lsif-docker/checkout.sh` to clone your repository to the `/source` directory in the
   Docker container's filesystem.

1. Modify `lsif-docker/gen_compile_commands.sh` to generate a compilation database
   (`compile_commands.json`).

   1. If you use autotools to build your project (`./autogen.sh && ./configure && make`), you can
      probably keep the existing contents.

   1. If you build your project using CMake, you can use `cmake -DCMAKE_EXPORT_COMPILE_COMMANDS=ON .`.

   1. If you use Bazel, you can use bazel-compilation-database:
      ```
      git clone --depth=10 https://github.com/grailbio/bazel-compilation-database.git /bazel-compilation-database
      /bazel-compilation-database/generate.sh
      ```

   1. If you use another build system or if any of the above steps break, consult this [very helpful
      guide to generating compilation databases for various build
      systems](https://sarcasm.github.io/notes/dev/compilation-database.html). It may be helpful to
      `docker build` your container and `docker run -it $IMAGE` to get an interactive shell into the
      container, so you can ensure the build environment is correct. We recommend getting the
      project to build normally first (e.g., emit a binary) and then following the aforementioned
      guide to modify the regular build steps to emit a compilation database.

      1. Most often, the `compile_commands.json` file will be emitted in the root directory of the
         repository. If this is not the case, you'll also need to modify `lsif-docker/gen_lsif.sh`
         to `cd` into the directory containing it and then run `lsif-clang --project-root=/source
         compile_commands.json`. If you're unsure of where `compile_commands.json` will be emitted,
         just continue to the next step for now.

1. Run `docker build lsif-docker` to build the Docker image.

1. Generate a Sourcegraph access token from your Sourcegraph instance (**Settings > Access
   tokens**). Give it `sudo` permission.

1. Run the following command to generate and upload LSIF data to Sourcegraph:
  ```
  docker run -e SRC_ACCESS_TOKEN=$ACCESS_TOKEN -e SRC_ENDPOINT=https://sourcegraph.example.com -e PROJECT_REV=HEAD $IMAGE_ID
  ```
  with the following substitutions:
  * `SRC_ACCESS_TOKEN=`: the Sourcegraph access token you just created
  * `SRC_ENDPOINT=`: the URL to your Sourcegraph instance
  * `PROJECT_REV=`: the revision of you repository to be indexed
  * `$IMAGE_ID`: the ID of the Docker image you just built

If successful, you should see the upload visible in [the repository settings page like
this](https://sourcegraph.com/github.com/tesseract-ocr/tesseract/-/settings/code-intelligence/uploads).

For reference, some examples of Dockerized C++ LSIF generation are:

* [`github.com/opencv/opencv`](https://github.com/sourcegraph/opencv/tree/master/docker)
* [`github.com/osquery/osquery`](https://github.com/sourcegraph/osquery/tree/master/lsif-docker)
* [`github.com/google/tcmalloc`](https://github.com/sourcegraph/tcmalloc/tree/master/docker)
* [`github.com/tesseract-ocr/tesseract`](https://github.com/sourcegraph/tesseract/tree/master/lsif-docker)

### Without Docker

It can sometimes be difficult to replicate the build environment inside a separate Docker
container. If this situation applies to you, you'll need to install `lsif-clang` directly to your
local dev environment.

1. Install `lsif-clang` in your environment using the [instructions in the `lsif-clang`
   repository](https://github.com/sourcegraph/lsif-clang/blob/llvmorg-10.0.0-lsif-clang/docs/install.md).

1. [Install the `src` CLI](https://github.com/sourcegraph/src-cli).

1. You'll need a way to generate a compilation database (i.e., a `compile_commands.json`
   file). There are different methods of doing so depending on your build tool, and we recommend
   reading [these excellent
   notes](https://sarcasm.github.io/notes/dev/compilation-database.html). If there isn't an explicit
   way to generate one with your build tool, we recommend using
   [Bear](https://github.com/rizsotto/Bear), which should be generic enough to handle any C++ build
   (but might be less efficient than explicit generation methods).

1. Generate the `compile_commands.json` file in the root directory of the repository.

1. Run `lsif-clang compile_commands.json` from the root directory. This should emit a `dump.lsif`
   file.

1. Run `src lsif upload` from the root directory. You may first have to [authenticate to your
   Sourcegraph instance](https://github.com/sourcegraph/src-cli#log-into-your-sourcegraph-instance).

If you run into issues along the way, a useful reference is [one of the
`Dockerfile`s](https://github.com/sourcegraph/tesseract/blob/master/lsif-docker/Dockerfile)
currently used for LSIF generation for an open-source repository.

## CI setup

Incorporating LSIF generation and uploading in CI will allow precise code navigation to remain
up-to-date without any human intervention.

If you created a `Dockerfile` that encapsulates LSIF generation, you can use the same one in your CI
pipeline.

If you installed `lsif-clang` directly into your host machine in development, you'll need to
incorporate those steps into your build scripts.

## Troubleshooting

### With Docker

If the `docker run` command fails, you likely have an error in one of the `lsif-docker/*.sh`
files. The general rule is if you can get your project to build normally (i.e., generate an
executable), you can get the LSIF indexer to generate LSIF. So we recommend the following approach
if things don't work on the first try:

1. Build the Docker image: `docker build lsif-docker`
1. Run the container with an interactive shell: `docker run -it $IMAGE_ID bash`
1. In the container shell, `cd /source` and figure out what steps are needed to build the
   project.
1. Once the build successfully completes, figure out which steps are needed to generate the
   `compile_commands.json` file. We have found [this
   guide](https://sarcasm.github.io/notes/dev/compilation-database.html) to be a useful resource.
1. Once you've successfully generated `compile_commands.json`, `cd` into the directory containing
   `compile_commands.json` and run `lsif-clang --project-root=/source compile_commands.json`. This
   should generate a `dump.lsif` file in the same directory. This `dump.lsif` should contain JSON
   describing all the symbols and references in the codebase (it should be rather large).
1. Once the `dump.lsif` file is generated correctly, set the environment variables
   `SRC_ACCESS_TOKEN` and `SRC_ENDPOINT` to the appropriate values in your shell. Then run `src lsif
   upload` from the directory containing the `lsif.dump` file. This should successfully upload the
   LSIF dump to Sourcegraph.
1. After you've successfully done all of the above in the container's interactive shell, incorporate
   these steps into the `lsif-docker/*.sh` files. Then re-build the Docker container and try running
   `docker run` again.
