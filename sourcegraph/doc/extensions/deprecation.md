# Sunsetting of Sourcegraph extensions

Sourcegraph extensions, originally released in 2018, create integrations with third parties and support some of our core features including code navigation (formally known as code intelligence).

As part of our commitment to enhance our platform and innovate to meet customer and market needs, we have been evaluating our current platform. In assessing our future vision for integrations and our current capabilities, we have decided to deprecate our current extensions framework.

We’re investing in a new model of integrations that allow deeper integration with our code intelligence platform and surfacing code context during the ideal moments in a developer’s workflow.

This decision means that after the September 2022 release of Sourcegraph or if you’re using Sourcegraph.com, **extensions will be impacted in the following way**:

- *If you’re on the September release or using Sourcegraph.com:*
  - A new feature flag will be introduced that turns on the extensions and the extension registry, this will be disabled by default.
  - You can no longer create new extensions on the public extension registry. If you have enabled the feature flag, you are able to create extensions on a private registry.
  - Top extensions (code navigation, git-extras, open-in-editor and search-exports) will become core to the functionality of the code intelligence platform

- *If you’re on an earlier release:*
  - You can no longer create new extensions on our public registry. If you’re using a private registry, you are unaffected.
  - Extensions and the extension registry will continue to work as expected.

Please note that this does not impact our IDE extensions, which will continue to allow you to search and navigate across all of your repositories without ever leaving your IDE or checking them out locally. Our Browser extensions will continue to have code navigation support, but will not provide other functionality to the code host (e.g. code coverage information).

Integrations continue to be an important part of our code intelligence platform and are looking forward to investing in an even more powerful framework for the future.
