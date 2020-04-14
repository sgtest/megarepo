# Creating a campaign from patches

A campaign can be created from a set of patches, one per repository. For each patch, a changeset (what code hosts call _pull request_ or _merge request_) will be created on the code host on which the repository is hosted.

Here is the short version for how to create a patch set and turn that into changesets by creating a campaign:

1. Create an action JSON file (e.g. `action.json`) that contains an action definition.
1. _Optional_: See repositories the action would run over: `src actions scope-query -f action.json`
1. Create a set of patches by executing the action over repositories: `src actions exec -f action.json > patches.json`
1. Save the patches in Sourcegraph by creating a patch set: `src campaign patchset create-from-patches < patches.json`
1. Create a campaign based on the patch set: `src campaigns create -branch=<branch-name> -patchset=<patchset-ID-returned-by-previous-command>`

Read on detailed steps and documentation.

## Requirements

If you have not done so already, first [install](https://github.com/sourcegraph/src-cli), [set up and configure](https://github.com/sourcegraph/src-cli#setup) the `src` CLI to point to your Sourcegraph instance.

## 1. Defining an action

The first thing we need is a definition of an "action". An action contains a list of steps to run in each repository returned by the results of the `scopeQuery` search string.

There are two types of steps: `docker` and `command`. See `src actions exec -help` for more information.

Here is an example of a multi-step action definition using the `docker` and `command` types:

```json
{
  "scopeQuery": "repo:go-* -repohasfile:INSTALL.md",
  "steps": [
    {
      "type": "command",
      "args": ["sh", "-c", "echo '# Installation' > INSTALL.md"]
    },
    {
      "type": "command",
      "args": ["sed", "-i", "", "s/No install instructions/See INSTALL.md/", "README.md"]
    },
    {
      "type": "docker",
      "dockerfile": "FROM alpine:3 \n CMD find /work -iname '*.md' -type f | xargs -n 1 sed -i s/this/that/g"
    },
    {
      "type": "docker",
      "image": "golang:1.13-alpine",
      "args": ["go", "fix", "/work/..."]
    }
  ]
}
```

This action will execute on every repository that has `go-` in its name and doesn't have an `INSTALL.md` file.

1. The first step (a `command` step) creates an `INSTALL.md` file in the root directory of each repository by running `sh` in a temporary copy of each repository. **This is executed on the machine on which `src` is being run.** Note that the first element in `"args"` is the command itself.

2. The second step, again a `"command"` step, runs the `sed` command to replace text in the `README.md` file in the root of each repository (the `-i ''` argument is only necessary for BSD versions of `sed` that usually come with macOS). Please note that the executed command is simply `sed` which means its arguments are _not_ expanded, as they would be in a shell. To achieve that, execute the `sed` as part of a shell invocation (using `sh -c` and passing in a single argument, for example, like in the first step).

3. The third step builds a Docker image from the specified `"dockerfile"` and starts a container with this image in which the repository is mounted under `/work`.

4. The fourth step starts a Docker container based on the `golang:1.13-alpine` image and runs `go fix /work/...` in it.

As you can see from these examples, the "output" of an action is the modified, local copy of a repository.

Save that definition in a file called `action.json` (or any other name of your choosing).

## 2. Executing an action to produce patches

With our action file defined, we can now execute it:

```sh
src actions exec -f action.json
```

This command is going to:

1. Build the required Docker image if necessary.
1. Download a ZIP archive of the repositories matched by the `"scopeQuery"` from the Sourcegraph instance to a local temporary directory in `/tmp`.
1. Execute the action for each repository in parallel (the number of parallel jobs can be configured with `-j`, the default is number of cores on the machine), with each step in an action being executed sequentially on the same copy of a repository. If a step in an action is of type `"command"` the command will be executed in the temporary directory that contains the copy of the repository. If the type is `"docker"` then a container will be launched in which the repository is mounted under `/work`.
1. Produce a patch for each repository with a diff between a fresh copy of the repository's contents and directory in which the action ran.

The output can either be saved into a file by redirecting it:

```sh
src actions exec -f action.json > patches.json
```

Or it can be piped straight into the next command we're going to use to save the patches on the Sourcegraph instance:

```sh
src actions exec -f action.json | src campaign patchset create-from-patches
```

>NOTE: **Where to run `src action exec`**

> The patches for a campaign are generated on the machine where the `src` CLI is executed, which in turn, downloads zip archives and runs each step against each repository. For most usecases we recommend that `src` CLI should be run on a Linux machine with considerable CPU, RAM, and network bandwidth to reduce the execution time. Putting this machine in the same network as your Sourcegraph instance will also improve performance.

> Another factor affecting execution time is the number of jobs executed in parallel, which is by default the number of cores on the machine. This can be adjusted using the `-j` parameter.

## 3. Creating a patch set from patches

The next step is to save the set of patches on the Sourcegraph instance so they can be turned into a campaign.

To do that, run:

```sh
src campaign patchset create-from-patches < patches.json
```

Or, again, pipe the patches directly into it:

```sh
src actions exec -f action.json | src campaign patchset create-from-patches
```

Once completed, the output will contain:

- The URL to preview the changesets that would be created on the code hosts.
- The command for the `src` SLI to create a campaign from the patch set.

## 4. Publishing a campaign

If you're happy with the preview of the campaign, it's time to trigger the creation of changesets (pull requests) on the code host(s) by creating and publishing the campaign:

```sh
src campaigns create -name='My campaign name' \
   -desc='My first CLI-created campaign' \
   -patchset=Q2FtcGFpZ25QbGFuOjg= \
   -branch=my-first-campaign
```

Creating this campaign will asynchronously create a pull request for each repository that has a patch in the patch set. You can check the progress of campaign completion by viewing the campaign on your Sourcegraph instance.

The `-branch` flag specifies the branch name that will be used for each pull request. If a branch with that name already exists for a repository, a fallback will be generated by appending a counter at the end of the name, e.g.: `my-first-campaign-1`.

If you have defined the `$EDITOR` environment variable, the configured editor will be used to edit the name and Markdown description of the campaign:

```sh
src campaigns create -patchset=Q2FtcGFpZ25QbGFuOjg= -branch=my-first-campaign
```
