# Sourcegraph extension architecture

The Sourcegraph extension API allows extension of Sourcegraph functionality through [specific extension points](https://unpkg.com/sourcegraph@24.7.0/dist/docs/index.html). The Sourcegraph extension architecture refers to the system which allows Sourcegraph client applications, such as the web application or browser extension, to communicate with Sourcegraph extensions. 

<object data="/dev/background-information/web/extension-architecture.svg" type="image/svg+xml" style="width:100%;">
</object>

## Glossary

| Term | Definition |
| --- | --- |
| Client application | Platform (e.g. web application) |
| Platform context | Platform-specific data and methods |
| Extension host | Worker thread in which extensions run |
| Extensions controller | Object which handles all communication between the client application and extensions |
| Extension | JavaScript file that imports `"sourcegraph"` and exports an `activate` function |


## Extension host bootstrapping

The following diagram depicts the process by which the extension host is initialized. You can click on a function signature to view its definition on Sourcegraph.

<object data="/dev/background-information/web/extension-host.svg" type="image/svg+xml" style="width:100%; height: 100%">
</object>

<!--- Update this diagram (../web/extension-host.drawio) on https://app.diagrams.net/  -->

Note that the extension host execution context varies depending on the client application:

| Client application | Extension host execution context |
| --- | --- |
| Sourcegraph web application | Web Worker |
| Browser extensions | A Web Worker spawned in the browser extension's background page for each content script instance. Messages are forwarded from the content script to its corresponding worker. |
| [Native Integration](../web/code_host_integrations.md#how-code-host-integrations-are-delivered) | Web Worker spawned in an `<iframe/>`. Messages are forwarded from the content script to the worker. |


## Inter-process communication

The client application runs on the main thread, while the extension host runs in a Web Worker, in a seperate global execution context. Under the hood, the client application and extension host communicate through messages, but the we rely on [comlink](https://github.com/GoogleChromeLabs/comlink), a proxy-based RPC library, in order to manage complexity and simplify implementation of new functionality. 


<!-- TODO(tj|p=2) future topics: 1) workbench views, 2) code tour/onboarding help 3) how to add APIs -->
