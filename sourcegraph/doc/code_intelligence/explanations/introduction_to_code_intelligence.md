# Introduction to code intelligence

Code intelligence is enabled by [Sourcegraph extensions](../../../extensions/index.md) and provides users with two different types of code intelligence: search-based and precise.

[**Search-based code intelligence**](./search_based_code_intelligence.md) works out of the box with all of the most popular programming languages via [extensions](https://sourcegraph.com/extensions?query=category%3A%22Programming+languages%22).

**Precise code intelligence** is an opt-in feature that is enabled in your admin settings and requires you to upload [LSIF data](../lsif_quickstart.md) for each repository to your Sourcegraph instance. Once setup is complete on Sourcegraph, precise code intelligence is available for use across popular development tools:

- On the Sourcegraph web UI
- On code files on your code host, via [integrations](../../../integration/index.md)
- On diffs in your code review tool, via integrations
- Via the [Sourcegraph API](https://docs.sourcegraph.com/api/graphql)

## Search-based vs. precise

[Search-based code intelligence](search_based_code_intelligence.md) is available by default on all Sourcegraph instances and provides fuzzy code intelligence using a combination of ctags and search. It is great for immediate access to code intelligence features, but due to its dependence on text-based search its results are considered imprecise.

[Precise code intelligence](precise_code_intelligence.md) returns metadata from a knowledge graph that is generated through code analysis. The precomputation step is used to generate the graph results in lookups that are fast and have a high degree of accuracy.

To learn more about how to work with each type of code intelligence, visit the [search-based](./search_based_code_intelligence.md) and [precise](./precise_code_intelligence.md) sections
