# Search examples

Below are examples that search repositories on [Sourcegraph.com](https://sourcegraph.com/search), our open source code search solution for GitHub and GitLab. You can copy and adapt the following search queries for use on your company’s private instance.

> See [**search query syntax**](../reference/queries.md) reference.

[Recent security-related changes on all branches](https://sourcegraph.com/search?q=type:diff+repo:github%5C.com/kubernetes/kubernetes%24+repo:%40*refs/heads/+after:"5+days+ago"+%5Cb%28auth%5B%5Eo%5D%5B%5Er%5D%7Csecurity%5Cb%7Ccve%7Cpassword%7Csecure%7Cunsafe%7Cperms%7Cpermissions%29&patternType=regexp)<br/>

```sgquery
type:diff repo:@*refs/heads/ after:"5 days ago"
\b(auth[^o][^r]|security\b|cve|password|secure|unsafe|perms|permissions)
```

[Admitted hacks and TODOs in app code](https://sourcegraph.com/search?q=-file:%5C.%28json%7Cmd%7Ctxt%29%24+hack%7Ctodo%7Ckludge%7Cfixme&patternType=regexp)<br/>

```sgquery
-file:\.(json|md|txt)$ hack|todo|kludge|fixme
```

[New usages of a function](https://sourcegraph.com/search?q=repo:github%5C.com/sourcegraph/+type:diff+after:%221+week+ago%22+%5C.subscribe%5C%28+lang:typescript&patternType=regexp)<br/>

```sgquery
type:diff after:"1 week ago" \.subscribe\( lang:typescript
```

[Find multiple terms in the same file, like testing of HTTP components](https://sourcegraph.com/search?q=repo:github%5C.com/sourcegraph/sourcegraph%24+%28test+AND+http+AND+NewRequest%29+lang:go&patternType=regexp)

```sgquery
repo:github\.com/sourcegraph/sourcegraph$ (test AND http AND NewRequest) lang:go
```

[Recent quality related changes on all branches (customize for your linters)](https://sourcegraph.com/search?q=repo:github%5C.com/sourcegraph/+repo:%40*refs/heads/:%5Emaster+type:diff+after:"1+week+ago"+%28eslint-disable%29&patternType=regexp)<br/>

```sgquery
repo:@*refs/heads/:^master type:diff after:"1 week ago" (eslint-disable)
```

[Recent dependency changes](https://sourcegraph.com/search?q=repo:github%5C.com/sourcegraph/+file:package.json+type:diff+after:%221+week+ago%22)<br/>

```sgquery
file:package.json type:diff after:"1 week ago"
```

[Files that are Apache licensed](https://sourcegraph.com/search?q=licensed+to+the+apache+software+foundation+select:file&patternType=literal)<br/>

```sgquery
licensed to the apache software foundation select:file
```

[Only _repositories_ with recent dependency changes](https://sourcegraph.com/search?q=repo:github%5C.com/sourcegraph/+file:package.json+type:diff+after:%221+week+ago%22+select:repo&patternType=regexp)

```sgquery
file:package.json type:diff after:"1 week ago" select:repo
```

## When to use regex search mode

Sourcegraph's default literal search mode is line-based and will not match across lines, so regex can be useful when you wish to do so:

[Matching multiple text strings in a file](https://sourcegraph.com/search?q=repo:%5Egithub%5C.com/Parsely/pykafka%24+Not+leader+for+partition&patternType=regexp)<br/>

```sgquery
repo:^github\.com/Parsely/pykafka$ Not leader for partition
```

Regex searches are also useful when searching boundaries that are not delimited by code structures:

[Finding css classes with word boundary regex](https://sourcegraph.com/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+%5Cbbtn-secondary%5Cb&patternType=regexp) <br />
```sgquery
repo:^github\.com/sourcegraph/sourcegraph$ \bbtn-secondary\b
```


## When to use structural search mode

Use structural search when you want to match code boundaries such as () or {}:

[Finding try catch statements with varying content](https://sourcegraph.com/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+try+%7B+:%5Bmatched_statements%5D+%7D+catch+%7B+:%5Bmatched_catch%5D+%7D&patternType=structural)<br/>
```sgquery
repo:^github\.com/sourcegraph/sourcegraph$
try { :[matched_statements] } catch { :[matched_catch] }
```
