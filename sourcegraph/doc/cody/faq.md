# Cody FAQ

<p class="subtitle">Find answers to the most common questions about Cody.</p>

## General

### Troubleshooting

See [Cody troubleshooting guide](troubleshooting.md).

### Does Cody train on my code?

Cody doesn't train on your code. Our third-party LLM providers do not train on your code either.

The way Cody generates an answer is the following:

- A user asks a question.
- Sourcegraph uses the code intelligence platform (search, code intelligence, embeddings) to retrieve code relevant to the question. In that process, permissions are enforced and only code that the user has read permission on is retrieved.
- Sourcegraph sends a prompt to an LLM to answer, providing the code retrieved as context.
- The reply is sent to Cody.

### Does Cody work with self-hosted Sourcegraph?

Yes, Cody works with self-hosted Sourcegraph instances, with the caveat that snippets of code (up to 28 KB per request) will be sent to a third party cloud service (Anthropic by default, but can also be OpenAI) on each request. Optionally, embeddings can be turned on for some repositories, which requires sending those repositories to another third party (OpenAI).

In particular, this means the Sourcegraph instance needs to be able to access the internet.

### Is there a public facing Cody API?

Not at the moment.

### Does Cody require Sourcegraph to function?

Yes. Sourcegraph is needed both to retrieve context and as a proxy for the LLM provider.

### What programming languages Cody supports?

- JavaScript
- TypeScript
- PHP
- Python
- Java
- C/C++
- C#
- Ruby
- Go
- Shell scripting languages (Bash, PowerShell, etc.)
- SQL
- Swift
- Objective-C
- Perl
- Rust
- Kotlin
- Scala
- Groovy
- R
- MATLAB
- Dart
- Lua
- Julia
- Cobol

## Embeddings

### What are embeddings for?

Embeddings are one of the many ways Sourcegraph uses to retrieve relevant code to feed the large language model as context. Embeddings / vector search are complementary to other strategies. While it matches really well semantically ("what is this code about, what does it do?"), it drops syntax and other important precise matching info. Sourcegraph's overall approach is to blend results from multiple sources to provide the best answer possible.

### When using embeddings, are permissions enforced? Does Cody get fed code that the users doesn't have access to?

Permissions are enforced when using embeddings. Today, Sourcegraph only uses embeddings search on a single repo, first checking that the users has access.

In the future, here are the steps that Sourcegraph will follow:

- determine which repo you have access to
- query embeddings for each of those repo
- pick the best results and send it back

### I scheduled a one-off embeddings job but it is not showing up in the list of jobs. What happened?

There can be several reasons why a job is not showing up in the list of jobs:

- The repository is already queued or being processed
- A job for the same repository and the same revision already completed successfully
- Another job for the same repository has been queued for processing within the [embeddings.MinimumInterval](./explanations/code_graph_context.md#adjust-the-minimum-time-interval-between-automatically-scheduled-embeddings) time window

### How do I stop a running embeddings job?

Jobs in state _QUEUED_ or _PROCESSING_ can be canceled by admins from the **Cody > Embeddings Jobs** page. To cancel a job, click on the _Cancel_ button of the job you want to cancel. The job will be marked for cancellation. Note that, depending on the state of the job, it might take a few seconds or minutes for the job to actually be canceled.

### What are the reasons files are skipped?

Files are skipped for the following reasons:

- The file is too large (1 MB)
- The file path matches an [exclusion pattern](./explanations/code_graph_context.md#excluding-files-from-embeddings)
- We have already generated more than [`embeddings.maxCodeEmbeddingsPerRepo`](./explanations/code_graph_context.md#limitting-the-number-of-embeddings-that-can-be-generated) or [`embeddings.maxTextEmbeddingsPerRepo`](./explanations/code_graph_context.md#limitting-the-number-of-embeddings-that-can-be-generated) embeddings for the repo.

## Third party dependencies

### What is the default `sourcegraph` provider for completions and embeddings?

The default `"provider": "sourcegraph"` for completions and embeddings is the [Sourcegraph Cody Gateway](./explanations/cody_gateway.md). Cody Gateway provides Sourcegraph enterprise instances access to completions and embeddings using third-party services like Anthropic and OpenAI.

### What third-party cloud services does Cody depend on today?

- Cody has one third-party dependency, which is Anthropic's Claude API. In the config, this can be replaced with OpenAI API.
- Cody can optionally use OpenAI to generate embeddings, that are then used to improve the quality of its context snippets, but this is not required.

The above is also the case even when using [the default `sourcegraph` provider, Cody Gateway](./explanations/cody_gateway.md), which uses the same third-party providers.

### What's the retention policy for Anthropic/OpenAI?

See our [terms](https://about.sourcegraph.com/terms/cody-notice).

### Can I use my own API keys?

Yes!

### Can I use with my Cloud IDE?

Yes, we support the following cloud development environments, Gitpod and GitHub Codespaces.

- [Gitpod instructions](https://www.gitpod.io/blog/boosting-developer-productivity-unleashing-the-power-of-sourcegraph-cody-in-gitpod)
- GitHub Codespaces
  - Open Codespaces
  - Install Cody AI by Sourcegraph
  - Copy the url in the browser, e.g. `vscode://sourcegraph.cody-ai...`
  - Open the Command Pallet <kbd>⌘ cmd/ctrl</kbd>+<kbd>shift</kbd>+<kbd>p</kbd> 
    - Choose `Developer: Open URL` and paste the URL, then press <kbd>return</kbd>/<kbd>enter</kbd>
