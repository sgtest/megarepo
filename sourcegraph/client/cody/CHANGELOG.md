# Changelog

All notable changes to Sourcegraph Cody will be documented in this file.

Starting from `0.2.0`, Cody is using `major.EVEN_NUMBER.patch` for release versions and `major.ODD_NUMBER.patch` for pre-release versions.

## [Unreleased]

### Added

- New recipe: `Generate PR description`. Generate the PR description using the PR template guidelines for the changes made in the current branch. [pull/51721](https://github.com/sourcegraph/sourcegraph/pull/51721)
- Open context search results links as workspace file [pull/52856](https://github.com/sourcegraph/sourcegraph/pull/52856)
- Cody Inline Assist: Decorations for `/fix` errors [pull/52796](https://github.com/sourcegraph/sourcegraph/pull/52796)

### Fixed

- Cody Inline Assist: Decorations for `/fix` on light theme [pull/52796](https://github.com/sourcegraph/sourcegraph/pull/52796)
- Cody Inline Assist: Fixs cody processing indefinitely isse [pull/52796](https://github.com/sourcegraph/sourcegraph/pull/52796)
- Cody Inline Assist: Use more than 1 context file for `/touch` [pull/52796](https://github.com/sourcegraph/sourcegraph/pull/52796)

### Changed

## [0.2.1]

### Fixed

- Escape Windows path separator in fast file finder path pattern [pull/52754](https://github.com/sourcegraph/sourcegraph/pull/52754)
- Only display errors from the embeddings clients for users connnected to an indexed codebase [pull/52780](https://github.com/sourcegraph/sourcegraph/pull/52780)

### Changed

## [0.2.0]

### Added

- Inline Assist recipe for creating new files with `/touch` command. [pull/52511](https://github.com/sourcegraph/sourcegraph/pull/52511)
- Cody completions: Experimental support for multi-line inline completions for JavaScript, TypeScript, Go, and Python using indentation based truncation. [issues/52588](https://github.com/sourcegraph/sourcegraph/issues/52588)
- Inline Assist recipe for creating new files [pull/52511](https://github.com/sourcegraph/sourcegraph/pull/52511)
- Display embeddings search, and connection error to the webview panel. [pull/52491](https://github.com/sourcegraph/sourcegraph/pull/52491)
- New recipe: `Optimize Code`. Optimize the time and space consumption of code. [pull/51974](https://github.com/sourcegraph/sourcegraph/pull/51974)
- Button to insert code block text at cursor position in text editor. [pull/52528](https://github.com/sourcegraph/sourcegraph/pull/52528)

### Fixed

- Cody completions: Fixed interop between spaces and tabs. [pull/52497](https://github.com/sourcegraph/sourcegraph/pull/52497)
- Fixes an issue where new conversations did not bring the chat into the foreground. [pull/52363](https://github.com/sourcegraph/sourcegraph/pull/52363)
- Cody completions: Prevent completions for lines that have a word in the suffix. [issues/52582](https://github.com/sourcegraph/sourcegraph/issues/52582)
- Cody completions: Fixes an issue where multi-line inline completions closed the current block even if it already had content. [pull/52615](https://github.com/sourcegraph/sourcegraph/52615)
- Cody completions: Fixed an issue where the Cody response starts with a newline and was previously ignored. [issues/52586](https://github.com/sourcegraph/sourcegraph/issues/52586)

### Changed

- Cody is now using `major.EVEN_NUMBER.patch` for release versions and `major.ODD_NUMBER.patch` for pre-release versions. [pull/52412](https://github.com/sourcegraph/sourcegraph/pull/52412)
- Cody completions: Fixed an issue where the Cody response starts with a newline and was previously ignored [issues/52586](https://github.com/sourcegraph/sourcegraph/issues/52586)
- Cody completions: Improves the behavior of the completions cache when characters are deleted from the editor. [pull/52695](https://github.com/sourcegraph/sourcegraph/pull/52695)

### Changed

- Cody completions: Improve completion logger and measure the duration a completion is displayed for. [pull/52695](https://github.com/sourcegraph/sourcegraph/pull/52695)

## [0.1.5]

### Added

### Fixed

- Inline Assist broken decorations for Inline-Fixup tasks [pull/52322](https://github.com/sourcegraph/sourcegraph/pull/52322)

### Changed

- Various Cody completions related improvements [pull/52365](https://github.com/sourcegraph/sourcegraph/pull/52365)

## [0.1.4]

### Added

- Added support for local keyword search on Windows. [pull/52251](https://github.com/sourcegraph/sourcegraph/pull/52251)

### Fixed

- Setting `cody.useContext` to `none` will now limit Cody to using only the currently open file. [pull/52126](https://github.com/sourcegraph/sourcegraph/pull/52126)
- Fixes race condition in telemetry. [pull/52279](https://github.com/sourcegraph/sourcegraph/pull/52279)
- Don't search for file paths if no file paths to validate. [pull/52267](https://github.com/sourcegraph/sourcegraph/pull/52267)
- Fix handling of embeddings search backward compatibility. [pull/52286](https://github.com/sourcegraph/sourcegraph/pull/52286)

### Changed

- Cleanup the design of the VSCode history view. [pull/51246](https://github.com/sourcegraph/sourcegraph/pull/51246)
- Changed menu icons and order. [pull/52263](https://github.com/sourcegraph/sourcegraph/pull/52263)
- Deprecate `cody.debug` for three new settings: `cody.debug.enable`, `cody.debug.verbose`, and `cody.debug.filter`. [pull/52236](https://github.com/sourcegraph/sourcegraph/pull/52236)

## [0.1.3]

### Added

- Add support for connecting to Sourcegraph App when a supported version is installed. [pull/52075](https://github.com/sourcegraph/sourcegraph/pull/52075)

### Fixed

- Displays error banners on all view instead of chat view only. [pull/51883](https://github.com/sourcegraph/sourcegraph/pull/51883)
- Surfaces errors for corrupted token from secret storage. [pull/51883](https://github.com/sourcegraph/sourcegraph/pull/51883)
- Inline Assist add code lenses to all open files [pull/52014](https://github.com/sourcegraph/sourcegraph/pull/52014)

### Changed

- Removes unused configuration option: `cody.enabled`. [pull/51883](https://github.com/sourcegraph/sourcegraph/pull/51883)
- Arrow key behavior: you can now navigate forwards through messages with the down arrow; additionally the up and down arrows will navigate backwards and forwards only if you're at the start or end of the drafted text, respectively. [pull/51586](https://github.com/sourcegraph/sourcegraph/pull/51586)
- Display a more user-friendly error message when the user is connected to sourcegraph.com and doesn't have a verified email. [pull/51870](https://github.com/sourcegraph/sourcegraph/pull/51870)
- Keyword context: Excludes files larger than 1M and adds a 30sec timeout period [pull/52038](https://github.com/sourcegraph/sourcegraph/pull/52038)

## [0.1.2]

### Added

- `Inline Assist`: a new way to interact with Cody inside your files. To enable this feature, please set the `cody.experimental.inline` option to true. [pull/51679](https://github.com/sourcegraph/sourcegraph/pull/51679)

### Fixed

- UI bug that capped buttons at 300px max-width with visible border [pull/51726](https://github.com/sourcegraph/sourcegraph/pull/51726)
- Fixes anonymous user id resetting after logout [pull/51532](https://github.com/sourcegraph/sourcegraph/pull/51532)
- Add error message on top of Cody's response instead of overriding it [pull/51762](https://github.com/sourcegraph/sourcegraph/pull/51762)
- Fixes an issue where chat input messages where not rendered in the UI immediately [pull/51783](https://github.com/sourcegraph/sourcegraph/pull/51783)
- Fixes an issue where file where the hallucination detection was not working properly [pull/51785](https://github.com/sourcegraph/sourcegraph/pull/51785)
- Aligns Edit button style with feedback buttons [pull/51767](https://github.com/sourcegraph/sourcegraph/pull/51767)

### Changed

- Pressing the icon to reset the clear history now makes sure that the chat tab is shown [pull/51786](https://github.com/sourcegraph/sourcegraph/pull/51786)
- Rename the extension from "Sourcegraph Cody" to "Cody AI by Sourcegraph" [pull/51702](https://github.com/sourcegraph/sourcegraph/pull/51702)
- Remove HTML escaping artifacts [pull/51797](https://github.com/sourcegraph/sourcegraph/pull/51797)

## [0.1.1]

### Fixed

- Remove system alerts from non-actionable items [pull/51714](https://github.com/sourcegraph/sourcegraph/pull/51714)

## [0.1.0]

### Added

- New recipe: `Codebase Context Search`. Run an approximate search across the codebase. It searches within the embeddings when available to provide relevant code context. [pull/51077](https://github.com/sourcegraph/sourcegraph/pull/51077)
- Add support to slash commands `/` in chat. [pull/51077](https://github.com/sourcegraph/sourcegraph/pull/51077)
  - `/r` or `/reset` to reset chat
  - `/s` or `/search` to perform codebase context search
- Adds usage metrics to the experimental chat predictions feature [pull/51474](https://github.com/sourcegraph/sourcegraph/pull/51474)
- Add highlighted code to context message automatically [pull/51585](https://github.com/sourcegraph/sourcegraph/pull/51585)
- New recipe: `Generate Release Notes` --generate release notes based on the available tags or the selected commits for the time period. It summarises the git commits into standard release notes format of new features, bugs fixed, docs improvements. [pull/51481](https://github.com/sourcegraph/sourcegraph/pull/51481)
- New recipe: `Generate Release Notes`. Generate release notes based on the available tags or the selected commits for the time period. It summarizes the git commits into standard release notes format of new features, bugs fixed, docs improvements. [pull/51481](https://github.com/sourcegraph/sourcegraph/pull/51481)

### Fixed

- Error notification display pattern for rate limit [pull/51521](https://github.com/sourcegraph/sourcegraph/pull/51521)
- Fixes issues with branch switching and file deletions when using the experimental completions feature [pull/51565](https://github.com/sourcegraph/sourcegraph/pull/51565)
- Improves performance of hallucination detection for file paths and supports paths relative to the project root [pull/51558](https://github.com/sourcegraph/sourcegraph/pull/51558), [pull/51625](https://github.com/sourcegraph/sourcegraph/pull/51625)
- Fixes an issue where inline code blocks were unexpectedly escaped [pull/51576](https://github.com/sourcegraph/sourcegraph/pull/51576)

### Changed

- Promote Cody from experimental to beta [pull/](https://github.com/sourcegraph/sourcegraph/pull/)
- Various improvements to the experimental completions feature

## [0.0.10]

### Added

- Adds usage metrics to the experimental completions feature [pull/51350](https://github.com/sourcegraph/sourcegraph/pull/51350)
- Updating `cody.codebase` does not require reloading VS Code [pull/51274](https://github.com/sourcegraph/sourcegraph/pull/51274)

### Fixed

- Fixes an issue where code blocks were unexpectedly escaped [pull/51247](https://github.com/sourcegraph/sourcegraph/pull/51247)

### Changed

- Improved Cody header and layout details [pull/51348](https://github.com/sourcegraph/sourcegraph/pull/51348)
- Replace `Cody: Set Access Token` command with `Cody: Sign in` [pull/51274](https://github.com/sourcegraph/sourcegraph/pull/51274)
- Various improvements to the experimental completions feature

## [0.0.9]

### Added

- Adds new experimental chat predictions feature to suggest follow-up conversations. Enable it with the new `cody.experimental.chatPredictions` feature flag. [pull/51201](https://github.com/sourcegraph/sourcegraph/pull/51201)
- Auto update `cody.codebase` setting from current open file [pull/51045](https://github.com/sourcegraph/sourcegraph/pull/51045)
- Properly render rate-limiting on requests [pull/51200](https://github.com/sourcegraph/sourcegraph/pull/51200)
- Error display in UI [pull/51005](https://github.com/sourcegraph/sourcegraph/pull/51005)
- Edit buttons for editing last submitted message [pull/51009](https://github.com/sourcegraph/sourcegraph/pull/51009)
- [Security] Content security policy to webview [pull/51152](https://github.com/sourcegraph/sourcegraph/pull/51152)

### Fixed

- Escaped HTML issue [pull/51144](https://github.com/sourcegraph/sourcegraph/pull/51151)
- Unauthorized sessions [pull/51005](https://github.com/sourcegraph/sourcegraph/pull/51005)

### Changed

- Various improvements to the experimental completions feature [pull/51161](https://github.com/sourcegraph/sourcegraph/pull/51161) [51046](https://github.com/sourcegraph/sourcegraph/pull/51046)
- Visual improvements to the history page, ability to resume past conversations [pull/51159](https://github.com/sourcegraph/sourcegraph/pull/51159)

## [Template]

### Added

### Fixed

### Changed
