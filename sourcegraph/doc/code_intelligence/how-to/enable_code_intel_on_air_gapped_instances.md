# Enable code intelligence on the air-gapped instances

Sourcegraph code intelligence [is implemented on top of extensions](../../dev/background-information/codeintel/extensions.md). Code-intel extensions are [enabled by default](../../extensions/usage.md#default-extensions). Sourcegraph extensions are fetched from [sourcegraph.com extensions registry](https://sourcegraph.com/extensions), so to make use of them the Sourcegraph instance should have Internet access and properly configured [`extensions.remoteRegistry`](../../admin/extensions/index.md#use-extensions-from-sourcegraphcom-or-disable-remote-extensions) site config setting. Using a proxy to access the [Sourcegraph extensions registry](https://sourcegraph.com/extensions) is not supported.

To make code intelligence work on the air-gapped Sourcegraph instances code intel extensions should be added to the instance's private extension registry following [this guide](https://github.com/sourcegraph/sourcegraph-extensions-cloning-scripts).

*Note:* To get the list of the programming languages code intelligence extensions run the following [src CLI](https://docs.sourcegraph.com/cli) command against sourcegraph.com
 ```
 src extensions list -query='sourcegraph/ category:"Programming languages"'
 ```
 
 Languages specifications for which [search-based code intelligence](../explanations/search_based_code_intelligence.md) is supported are listed [here](https://github.com/sourcegraph/code-intel-extensions/blob/master/template/src/language-specs/languages.ts#L345-L390).
