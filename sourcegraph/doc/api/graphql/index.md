# Sourcegraph GraphQL API

The Sourcegraph GraphQL API is a rich API that exposes data related to the code available on a Sourcegraph instance.

The Sourcegraph GraphQL API supports the following types of queries:

- Full-text and regexp code search
- Rich git-level metadata, including commits, branches, blame information, and file tree data
- Repository and user metadata

> NOTE: The API is under active development. Backwards compatibility will be maintained in most
> cases, and backwards-incompatible changes will be documented in the
> [changelog](https://sourcegraph.com/github.com/sourcegraph/sourcegraph/-/blob/CHANGELOG.md).

## Quickstart

Generate an access token on your Sourcegraph instance at:

```none
https://sourcegraph.example.com/user/settings/tokens
```

Then run this query to echo your username back:

<!--
  DO NOT CHANGE THIS TO A CODEBLOCK.
  We want line breaks for readability, but backslashes to escape them do not work cross-platform.
  This uses line breaks that are rendered but not copy-pasted to the clipboard.
-->
<pre class="pre-wrap"><code>curl<span class="virtual-br"></span> -H 'Authorization: token YOUR_TOKEN'<span class="virtual-br"></span> -d '{"query": "query { currentUser { username } }"}'<span class="virtual-br"></span>https://sourcegraph.example.com/.api/graphql</code></pre>

You should see a response like this:

```json
{ "data": { "currentUser": { "username": "YOUR_USERNAME" } } }
```

## Documentation & tooling

### API Console

Sourcegraph includes a built-in API console that lets you write queries and view API documentation in your browser.

You can find the API console at any time by clicking **your profile picture** and clicking **API console** in the bottom left of the page (next to **Sign out**), or by visiting it directly at `https://sourcegraph.example.com/api/console`.

If you have not yet set up a Sourcegraph server, you can also test out the API on the [Sourcegraph.com API console](https://sourcegraph.com/api/console) (which always uses the latest version of the API).

### Documentation

Sourcegraph's GraphQL API documentation is available directly in the API console itself. To access the documentation, click **Docs** on the right-hand side of the API console page.

### Search

See [additional documentation about search GraphQL API](search.md).

### Sudo access tokens

Site admins may create access tokens with the special `site-admin:sudo` scope, which allows the holder to perform any action as any other user.

<!--
  DO NOT CHANGE THIS TO A CODEBLOCK.
  We want line breaks for readability, but backslashes to escape them do not work cross-platform.
  This uses line breaks that are rendered but not copy-pasted to the clipboard.
-->
<pre class="pre-wrap"><code>curl<span class="virtual-br"></span> -H 'Authorization: token-sudo user="SUDO_TO_USERNAME",token="YOUR_TOKEN"'<span class="virtual-br"></span> -d '{"query": "query { currentUser { username } }"}'<span class="virtual-br"></span> https://sourcegraph.example.com/.api/graphql</code></pre>

This scope is useful when building Sourcegraph integrations with external services where the service needs to communicate with Sourcegraph and does not want to force each user to individually authenticate to Sourcegraph.

### Using the API via the Sourcegraph CLI

A command line interface to Sourcegraph's API is available. Today, it is roughly the same as using the API via `curl` (see below), but it offers a few nice things:

- Allows you to easily compose queries from scripts, e.g. without worrying about escaping JSON input to `curl` properly.
- Reads your access token and Sourcegraph server endpoint from a config file (or env var).
- Pipe multi-line GraphQL queries into it easily.
- Get any API query written using the CLI as a `curl` command using the `src api -get-curl` flag.

To learn more, see [github.com/sourcegraph/src-cli](https://github.com/sourcegraph/src-cli)

### Using the API via curl

The entire API can be used via `curl` (or any HTTP library), just the same as any other GraphQL API. For example:

<!--
  DO NOT CHANGE THIS TO A CODEBLOCK.
  We want line breaks for readability, but backslashes to escape them do not work cross-platform.
  This uses line breaks that are rendered but not copy-pasted to the clipboard.
-->
<pre class="pre-wrap"><code>curl<span class="virtual-br"></span> -H 'Authorization: token YOUR_TOKEN'<span class="virtual-br"></span> -d '{"query":"query($query: String!) { search(query: $query) { results { resultCount } } }","variables":{"query":"Router"}}'<span class="virtual-br"></span> https://sourcegraph.com/.api/graphql</code></pre>

i.e. you just need to send the `Authorization` header and a JSON object like `{"query": "my query string", "variables": {"var1": "val1"}}`.

## Examples

See "[Sourcegraph GraphQL API examples](examples.md)".
