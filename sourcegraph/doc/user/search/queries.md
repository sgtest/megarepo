# Search query syntax

<style>
tr td:nth-child(3) {
  min-width: 250px;
}
tr td:nth-child(3) code {
  word-break: break-all;
}
</style>

Search queries can consist of just words, and you'll see results where those words appear in order, in all files across all repositories. Many queries will also use keywords. Keywords help to filter searches, define the type of search, and more. This page is a comprehensive list of keywords available for code search.

As of version 3.9.0, by default, searches are interpreted literally instead of as regexp. Site admins and users can change their instance and personal default behavior by changing the `search.defaultPatternType` setting to "literal" or "regexp". To toggle regexp search, you can click the dot-star icon in the search input, or use the `patterntype:` keyword in your search.

## Keywords (all searches)

The following keywords can be used on all searches (using [RE2 syntax](https://golang.org/s/re2syntax) any place a regex is accepted):

| Keyword | Description | Examples |
| --- | --- | --- |
| **any-string**| Strings are matched exactly, including whitespace and punctuation. | [`open(props`](https://sourcegraph.com/search?q=open%28props&patternType=literal) |
| **patterntype:literal, patterntype:structural, patterntype:regexp**  | Configure your query to be interpreted literally, as a regular expression, or a [structural search pattern](structural.md).| [`test. patternType:literal`](https://sourcegraph.com/search?q=test.+patternType:literal)<br/>[`(open\|close)file patternType:regexp`](https://sourcegraph.com/search?q=%28open%7Cclose%29file&patternType=regexp) |
| **"any string"**  | When using `patterntype:regexp`, double-quote a string to find exact matches. Supports `\"` and `\\` escapes. | [`"*string" patternType:regexp`](https://sourcegraph.com/search?q=%22*string%22&patternType=regexp) |
| **repo:regexp-pattern** <br> **repo:regexp-pattern@rev** <br> _alias: r_  | Only include results from repositories whose path matches the regexp. A repository's path is a string such as _github.com/myteam/abc_ or _code.example.com/xyz_ that depends on your organization's repository host. If the regexp ends in **@rev**, that revision is searched instead of the default branch (usually `master`).  | [`repo:gorilla/mux testroute`](https://sourcegraph.com/search?q=repo:gorilla/mux+testroute)<br/>`repo:alice/abc@mybranch`  |
| **-repo:regexp-pattern** <br> _alias: -r_ | Exclude results from repositories whose path matches the regexp. | `repo:alice/ -repo:old-repo` |
| **repogroup:group-name** <br> _alias: g_ | Only include results from the named group of repositories (defined by the server admin). Same as using a repo: keyword that matches all of the group's repositories. Use repo: unless you know that the group exists. | |
| **file:regexp-pattern** <br> _alias: f_ | Only include results in files whose full path matches the regexp. | [`file:\.js$ httptest`](https://sourcegraph.com/search?q=file:%5C.js%24+httptest) <br> [`file:internal/ httptest`](https://sourcegraph.com/search?q=file:internal/+httptest) |
| **-file:regexp-pattern** <br> _alias: -f_ | Exclude results from files whose full path matches the regexp. | [`file:\.js$ -file:test http`](https://sourcegraph.com/search?q=file:%5C.js%24+-file:test+http) |
| **lang:language-name** <br> _alias: l_ | Only include results from files in the specified programming language. | [`lang:typescript encoding`](https://sourcegraph.com/search?q=lang:typescript+encoding) |
| **-lang:language-name** <br> _alias: -l_ | Exclude results from files in the specified programming language. | [`-lang:typescript encoding`](https://sourcegraph.com/search?q=-lang:typescript+encoding) |
| **type:symbol** | Perform a symbol search. | [`type:symbol path`](https://sourcegraph.com/search?q=type:symbol+path)  ||
| **case:yes**  | Perform a case sensitive query. Without this, everything is matched case insensitively. | [`OPEN_FILE case:yes`](https://sourcegraph.com/search?q=OPEN_FILE+case:yes) |
| **fork:no, fork:only** | Filter out results from repository forks or filter results to only repository forks. | [`fork:no repo:sourcegraph`](https://sourcegraph.com/search?q=fork:no+repo:sourcegraph) |
| **archived:no, archived:only** | Filter out results from archived repositories or filter results to only archived repositories. By default, results from archived repositories are included. | [`repo:sourcegraph/ archived:only`](https://sourcegraph.com/search?q=repo:%5Egithub.com/sourcegraph/+archived:only) |
| **repohasfile:regexp-pattern** | Only include results from repositories that contain a matching file. This keyword is a pure filter, so it requires at least one other search term in the query.  Note: this filter currently only works on text matches and file path matches. | [`repohasfile:\.py file:Dockerfile pip`](https://sourcegraph.com/search?q=repohasfile:%5C.py+file:Dockerfile+pip+repo:/sourcegraph/) |
| **-repohasfile:regexp-pattern** | Exclude results from repositories that contain a matching file. This keyword is a pure filter, so it requires at least one other search term in the query. Note: this filter currently only works on text matches and file path matches. | [`-repohasfile:Dockerfile docker`](https://sourcegraph.com/search?q=-repohasfile:Dockerfile+docker) |
| **repohascommitafter:"string specifying time frame"** | (Experimental) Filter out stale repositories that don't contain commits past the specified time frame. | [`repohascommitafter:"last thursday"`](https://sourcegraph.com/search?q=error+repohascommitafter:%22last+thursday%22) <br> [`repohascommitafter:"june 25 2017"`](https://sourcegraph.com/search?q=error+repohascommitafter:%22june+25+2017%22) |
| **count:_N_**<br/> | Retrieve at least <em>N</em> results. By default, Sourcegraph stops searching early and returns if it finds a full page of results. This is desirable for most interactive searches. To wait for all results, or to see results beyond the first page, use the **count:** keyword with a larger <em>N</em>. This can also be used to get deterministic results and result ordering (whose order isn't dependent on the variable time it takes to perform the search). | [`count:1000 function`](https://sourcegraph.com/search?q=count:1000+repo:sourcegraph/sourcegraph$+function) |
| **timeout:_go-duration-value_**<br/> | Customizes the timeout for searches. The value of the parameter is a string that can be parsed by the [Go time package's `ParseDuration`](https://golang.org/pkg/time/#ParseDuration) (e.g. 10s, 100ms). By default, the timeout is set to 10 seconds, and the search will optimize for returning results as soon as possible. The timeout value cannot be set longer than 1 minute. When provided, the search is given the full timeout to complete. | [`repo:^github.com/sourcegraph timeout:15s func count:10000`](https://sourcegraph.com/search?q=repo:%5Egithub.com/sourcegraph/+timeout:15s+func+count:10000) |

Multiple or combined **repo:** and **file:** keywords are intersected. For example, `repo:foo repo:bar` limits your search to repositories whose path contains **both** _foo_ and _bar_ (such as _github.com/alice/foobar_). To include results from repositories whose path contains **either** _foo_ or _bar_, use `repo:foo|bar`.

---

## Keywords (diff and commit searches only)

The following keywords are only used for **commit diff** and **commit message** searches, which show changes over time:

| Keyword  | Description | Examples |
| --- | --- | --- |
| **repo:regexp-pattern@refs** | Specifies which Git refs (`:`-separated) to search for commits. Use `*refs/heads/` to include all Git branches (and `*refs/tags/` to include all Git tags). You can also prefix a Git ref name or pattern with `^` to exclude. For example, `*refs/heads/:^refs/heads/master` will match all commits that are not merged into master. | [`repo:vscode@*refs/heads/:^refs/heads/master type:diff task`](https://sourcegraph.com/search?q=repo:%5Egithub%5C.com/Microsoft/vscode%24%40*refs/heads/:%5Erefs/heads/master+type:diff+after:%221+month+ago%22+task#1) (unmerged commit diffs containing `task`) |
| **type:diff** <br> **type:commit**  | Specifies the type of search. By default, searches are executed on all code at a given point in time (a branch or a commit). Specify the `type:` if you want to search over changes to code or commit messages instead (diffs or commits).  | [`type:diff func`](https://sourcegraph.com/search?q=type:diff+func+repo:sourcegraph/sourcegraph$) <br> [`type:commit test`](https://sourcegraph.com/search?q=type:commit+test+repo:sourcegraph/sourcegraph$) |
| **author:name** | Only include results from diffs or commits authored by the user. Regexps are supported. Note that they match the whole author string of the form `Full Name <user@example.com>`, so to include only authors from a specific domain, use `author:example.com>$`.<br><br> You can also search by `committer:git-email`. _Note: there is a committer only when they are a different user than the author._ | [`type:diff author:nick`](https://sourcegraph.com/search?q=repo:sourcegraph/sourcegraph$+type:diff+author:nick) |
| **before:"string specifying time frame"** | Only include results from diffs or commits which have a commit date before the specified time frame | [`before:"last thursday"`](https://sourcegraph.com/search?q=repo:sourcegraph/sourcegraph$+type:diff+author:nick+before:%22last+thursday%22) <br> [`before:"november 1 2019"`](https://sourcegraph.com/search?q=repo:sourcegraph/sourcegraph$+type:diff+author:nick+before:%22november+1+2019%22) |
| **after:"string specifying time frame"**  | Only include results from diffs or commits which have a commit date after the specified time frame| [`after:"6 weeks ago"`](https://sourcegraph.com/search?q=repo:sourcegraph/sourcegraph$+type:diff+author:nick+after:%226+weeks+ago%22) <br> [`after:"november 1 2019"`](https://sourcegraph.com/search?q=repo:sourcegraph/sourcegraph$+type:diff+author:nick+after:%22november+1+2019%22) |
| **message:"any string"** | Only include results from diffs or commits which have commit messages containing the string | [`type:commit message:"testing"`](https://sourcegraph.com/search?q=type:commit+repo:sourcegraph/sourcegraph$+message:%22testing%22) <br> [`type:diff message:"testing"`](https://sourcegraph.com/search?q=type:diff+repo:sourcegraph/sourcegraph$+message:%22testing%22) |

## Repository name search

A query with only `repo:` filters returns a list of repositories with matching names.

Example: [`repo:docker repo:registry`](https://sourcegraph.com/search?q=repo:docker+repo:registry)

## Filename search

A query with `type:path` restricts terms to matching filenames only (not file contents).

Example: [`type:path repo:/docker/ registry`](https://sourcegraph.com/search?q=type:path+repo:/docker/+registry)
