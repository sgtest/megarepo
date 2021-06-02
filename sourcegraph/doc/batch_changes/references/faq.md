# FAQ

This is a compilation of some common questions about Batch Changes.

### What happens if my batch change creation breaks down at 900 changesets out of 1,000? Do I have to re-run it all again?
Batch Changes' default behavior is to stop if creating the diff on a repo errors. You can choose to ignore errors instead by adding the [`-skip-errors`](../../cli/references/batch/preview.md) flag to the `src batch preview` command.

### Can we close a batch change and still leave the changesets open?
Yes. There is a confirmation page that shows you all the actions that will occur on the various changesets in the batch change after you close it. Open changesets will be marked 'Kept open', which means that batch change won't alter them. See [closing a batch change](../how-tos/closing_or_deleting_a_batch_change.md#closing-a-batch-change).

### How scalable is Batch Changes? How many changesets can I create?
Batch Changes can create tens of thousands of changesets. This is something we run testing on internally.
Known limitations:

- Since diffs are created locally by running a docker container, performance depends on the capacity of your machine. See [How `src` executes a batch spec](../explanations/how_src_executes_a_batch_spec.md).
- Batch Changes creates changesets in parallel locally. You can set up the maximum number of parallel jobs with [`-j`](../../cli/references/batch/apply.md)
- Manipulating (commenting, notifying users, etc) changesets at that scale can be clumsy. This is a major area of work for future releases.


### How long does it take to create a batch change?
A rule of thumb:

- measure the time it takes to run your change container on a typical repository
- multiply by the number of repositories
- divide by the number of changeset creation jobs that will be ran in parallel set by the [`-j`](../../cli/references/batch/apply.md) CLI flag. It defaults to GOMAXPROCS, [roughly](https://golang.org/pkg/runtime/#NumCPU) the number of available cores.

Note: If you run memory-intensive jobs, you might need to reduce the number of parallel job executions. You can run `docker stats` locally to get an idea of memory usage.

### My batch change does not open changesets on all the repositories it should. Why?
- Do you have enough permissions? Batch Changes will error on the repositories you don’t have access to. See [Repository permissions for Batch Changes](../explanations/permissions_in_batch_changes.md).
- Does your `repositoriesMatchingQuery` contain all the necessary flags? If you copied the query from the sourcegraph UI, note that some flags are represented as buttons (case sensitivity, regex, structural search), and do not appear in the query unless you use the copy query button.

### Can I create tickets or issues along with Batch Changes?
Batch Changes does not support a declarative syntax for issues or tickets.
However, [steps](../references/batch_spec_yaml_reference.md#steps-run) can be used to run any container. Some users have built scripts to create tickets at each apply:

- [Jira tickets](https://github.com/sourcegraph/batch-change-examples/tree/main/jira-tickets)
- [GitHub issues](https://github.com/sourcegraph/batch-change-examples/tree/main/github-issues)

### What happens to the preview page if the batch spec is not applied?
Unapplied batch specs are removed from the database after 7 days.

### Can I pull containers from private container registries in a batch change?
Yes. When [executing a batch spec](../explanations/how_src_executes_a_batch_spec.md), `src` will attempt to pull missing docker images. If you are logged into the private container registry, it will pull from it. Also see [`steps.container`](batch_spec_yaml_reference.md#steps-container). Within the spec, if `docker pull` points to your private registry from the command line, it will work as expected. 

However, outside of the spec, `src` pulls an image from Docker Hub when running in volume workspace mode. This is the default on macOS, so you will need to use one of the following three workarounds:

1. Run `src` with the `-workspace bind` flag. This will be slower, but will prevent `src` from pulling the iamge.
2. If you have a way of replicating trusted images onto your private registry, you can replicate [our image](https://hub.docker.com/r/sourcegraph/src-batch-change-volume-workspace) to your private registry. Ensure that the replicated image has the same tags, or this will fail.
3. If you have the ability to ad hoc pull images from public Docker Hub, you can run `docker pull -a sourcegraph/src-batch-change-volume-workspace` to pull the image and its tags.

> NOTE: If you choose to replicate or pull the Docker image, you should ensure that it is frequently synchronized, as a new tag is pushed each time `src` is released.

### What tool can I use for changing/refactoring `<programming-language>`?

Batch Changes supports any tool that can run in a container and changes file contents on disk. You can use the tool/script that works for your stack or build your own, but here is a list of [examples](https://github.com/sourcegraph/batch-change-examples) to get started.
Common language agnostic starting points:

- `sed`, [`yq`](https://github.com/mikefarah/yq), `awk` are common utilities for changing text
- [comby](https://comby.dev/docs/overview) is a language-aware structural code search and replace tool. It can match expressions and function blocks, and is great for more complex changes.
