# Sourcegraph browser extension

[![code style: prettier](https://img.shields.io/badge/code_style-prettier-ff69b4.svg)](https://github.com/prettier/prettier)
![license](https://img.shields.io/badge/license-MIT-blue.svg)

[![chrome version](https://img.shields.io/chrome-web-store/v/dgjhfomjieaadpoljlnidmbgkdffpack.svg?logo=Google%20Chrome&logoColor=white)](https://chrome.google.com/webstore/detail/sourcegraph/dgjhfomjieaadpoljlnidmbgkdffpack)
[![chrome users](https://img.shields.io/chrome-web-store/users/dgjhfomjieaadpoljlnidmbgkdffpack.svg)](https://chrome.google.com/webstore/detail/sourcegraph/dgjhfomjieaadpoljlnidmbgkdffpack)
[![chrome rating](https://img.shields.io/chrome-web-store/rating/dgjhfomjieaadpoljlnidmbgkdffpack.svg)](https://chrome.google.com/webstore/detail/sourcegraph/dgjhfomjieaadpoljlnidmbgkdffpack)\

## Overview

The Sourcegraph browser extension adds tooltips to code on GitHub, Phabricator, and Bitbucket.
The tooltips include features like:

- symbol type information & documentation
- go to definition & find references (currently for Go, Java, TypeScript, JavaScript, Python)
- find references

#### 🚀 Install: [**Sourcegraph for Chrome**](https://chrome.google.com/webstore/detail/sourcegraph/dgjhfomjieaadpoljlnidmbgkdffpack)

#### 🚀 Install: [**Sourcegraph for Firefox**](https://docs.sourcegraph.com/integration/browser_extension)

It works as follows:

- when visiting e.g. https://github.com/..., the extension injects a content script (inject.bundle.js)
- there is a background script running to access certain chrome APIs, like storage (background.bundle.js)
- a "code view" contains rendered (syntax highlighted) code (in an HTML table); the extension adds event listeners to the code view which control the tooltip
- when the user mouses over a code table cell, the extension modifies the DOM node:
  - text nodes are wrapped in `<span>` (so hover/click events have appropriate specificity)
  - element nodes may be recursively split into multiple element nodes (e.g. a `<span>&Router{namedRoutes:<span>` contains multiple code tokens, and event targets need more granular ranges)
  - We assume syntax highlighting takes care of the base case of wrapping a discrete language symbol
  - tooltip data is fetched from the Sourcegraph API
- when an event occurs, we modify a central state store about what kind of tooltip to display
- code subscribes to the central store updates, and creates/adds/removes/hides an absolutely positioned element (the tooltip)

## Project layout

- `src/`
  - `browser-extension/`
    Entrypoint for browser extension builds. (Includes bundled assets, background scripts, options)
    - `web-extension-api/`
      [A wrapper around the web extension APIs.](./src/browser-extension/web-extension-api/README.md)
  - `native-integration/`
    Entrypoint for the native code host integrations (Phabricator, Gitlab and Bitbucket).
  - `shared/`
    Code shared by the browser extension and the native integrations. Ideally, nothing in here should reach into any other directory.
    - `code-hosts/`
      Contains the implementations of code-host specific features for each supported code host.
      - `shared/`
        Code shared between multiple code hosts.
  - `config/`
    Configuration code that is bundled via webpack. The configuration code adds properties to `window` that make it easier to tell what environment the script is running in. This is useful because the code can be run in the content script, background, options page, or in the actual page when injected by Phabricator and each environment will have different ways to do different things.
  - `end-to-end/`
    E2E test suite.
- `scripts/`
  Build scripts.
- `config/`
  Build configs.
- `build/`
  Generated directory containing the output from webpack and the generated bundles for each browser.

## Requirements

- `node`
- `yarn`
- `make`

## Development

For each browser run:

```bash
yarn run dev
```

To only build for a single browser (which makes builds faster in local development), set the env var `TARGETS=chrome` or `TARGETS=firefox`.

Now, follow the steps below for the browser you intend to work with.

### Chrome

- Browse to [chrome://extensions](chrome://extensions).
- If you already have the Sourcegraph extension installed, disable it by unchecking the "Enabled" box.
- Click on [Load unpacked extensions](https://developer.chrome.com/extensions/getstarted#unpacked), and select the `build/chrome` folder.
- Browse to any public repository on GitHub to confirm it is working.
- After making changes it is necessary to refresh the extension. This is done by going to [chrome://extensions](chrome://extensions) and clicking "Reload".

![Add dist folder](readme-load-extension-asset.png)

#### Updating the bundle

Click reload for Sourcegraph at `chrome://extensions`

### Firefox (hot reloading)

In a separate terminal session run:

```bash
yarn global add web-ext
yarn run dev:firefox
```

A Firefox window will be spun up with the extension already installed.

#### Updating the bundle

Save a file and wait for webpack to finish rebuilding.

#### Caveats

The window that is spun up is completely separate from any existing sessions you have on Firefox.
You'll have to sign into everything at the beginning of each development session(each time you run `yarn run dev:firefox`).
You should ensure you're signed into any Sourcegraph instance you point the extension at as well as GitHub.

### Firefox (manual)

- Go to `about:debugging`
- Select "Enable add-on debugging"
- Click "Load Temporary Add-on" and select "firefox-bundle.xpi"
- [More information](https://developer.mozilla.org/en-US/docs/Tools/about:debugging#Add-ons)

#### Updating the bundle

Click reload for Sourcegraph at `about:debugging`

## Testing

- Unit tests: `yarn test`
- E2E tests: `yarn test-e2e`

### e2e tests

The test suite in `end-to-end/github.test.ts` runs on the release branch `bext/release` in both Chrome and Firefox against a Sourcegraph Docker instance.

The test suite in end-to-end/phabricator.test.ts tests the Phabricator native integration.
It assumes an existing Sourcegraph and Phabricator instance that has the Phabricator extension installed.
There are automated scripts to set up the Phabricator instance, see https://docs.sourcegraph.com/dev/phabricator_gitolite.
It currently does not run in CI and is intended to be run manually for release testing.

`end-to-end/bitbucket.test.ts` tests the browser extension on a Bitbucket Server instance.

`end-to-end/gitlab.test.ts` tests the browser extension on gitlab.com (or a private Gitlab instance).

## Deploy

Deployment the Chrome web store happen automatically in CI when the `bext/release` branch is updated.
Releases are also uploaded to the [GitHub releases
page](https://github.com/sourcegraph/browser-extensions/releases) and tagged in
git.

To release the latest commit on master, ensure your master is up-to-date and run

```sh
git push origin master:bext/release
```
