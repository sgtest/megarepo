# Code intelligence features

Using our [integrations](../../../integration/index.md), all code intelligence features are available everywhere you read code! This includes in browsers and GitHub pull requests.

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/code-intelligence/extension-example.gif" width="450" style="margin-left:0;margin-right:0;"/>

## Popover

Popovers allow you to quickly glance at the type signature and accompanying documentation of a symbol definition without having to context switch to another source file (which may or may not be available while browsing code).

<img src="../img/popover.png" width="500"/>

## Go to definition

When you click on the 'Go to definition' button in the popover or click on a symbol's name (in the sidebar or code view), you will be navigated directly to the definition of the symbol.

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/code-intelligence/go-to-def.gif" width="500"/>

## Find references

When you select 'Find references' from the popover, a panel will be shown at the bottom of the page that lists all of the references found for both precise (LSIF or language server) and search-based results (from search heuristics). This panel will separate references by repository, and you can optionally group them by file.

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/code-intelligence/find-refs.gif" width="450"/>

> NOTE: When a particular token returns a large number of references, we truncate the results to < 500 to optimize for browser loading speed. We are planning to improve this in the future with the ability to view it as a search so that users can utilize the powerful filtering of Sourcegraph's search to find the references they are looking for.

## Dependency navigation

If [auto-indexing](auto_indexing.md) is enabled for your instance, you will also be able to Find references and navigate precisely across your dependencies. 

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/code-intelligence/dependency-nav.gif" width="500"/>

> NOTE: This feature is in Experimental phase and currently available for Go, Java, Scala, Kotlin packages only. We plan to expand language support for this feature in the future.

## Find implementations

If precise code intelligence is enabled for your repositories, you can click on “Find Implementations” to navigate to a symbol’s interface definition. If you’re at the interface definition itself, clicking on “Find Implementations” will show all the places where the interface is being implemented, allowing you to explore how it’s being used by other users across repositories. It can also show which interfaces a struct implements.

<img src="https://storage.googleapis.com/sourcegraph-assets/docs/images/code-intelligence/find-impl.gif" width="450"/>

> NOTE: Currently available for Go repositories only. We plan to expand language support for this feature in the future.

## Symbol search

We use [Ctags](https://github.com/universal-ctags/ctags) to index the symbols of a repository on-demand. These symbols are used to implement symbol search, which will match declarations instead of plain-text.

<img src="../img/Symbols.png" width="500"/>

### Symbol sidebar

We use [Ctags](https://github.com/universal-ctags/ctags) to index the symbols of a repository on-demand. These symbols are also used for the symbol sidebar, which categorizes declarations by type (variable, function, interface, etc). Clicking on a symbol in the sidebar jumps you to the line where it is defined.

<img src="../img/SymbolSidebar.png" width="500"/>
