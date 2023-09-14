# Sourcegraph Changelog

## [Unreleased]

### Added

- New settings layout [#56579](https://github.com/sourcegraph/sourcegraph/pull/56579)
- New settings to enable debugging with the agent [#55821](https://github.com/sourcegraph/sourcegraph/pull/55821)
- Added ability to hide completion suggestions with ESC key [#55955](https://github.com/sourcegraph/sourcegraph/pull/55955)
- New alt-backslash shortcut to exlicitly trigger autocomplete [#55926](https://github.com/sourcegraph/sourcegraph/pull/55926)
- Add visual hints about Cody status to status bar [#56046](https://github.com/sourcegraph/sourcegraph/pull/56046)
- Added a status bar toggle for enabling/disabling Cody autocomplete [#56310](https://github.com/sourcegraph/sourcegraph/pull/56310)
- New settings to enable/disable autocomplete for individual languages [#56411](https://github.com/sourcegraph/sourcegraph/pull/56411)
- New onboarding panel is being displayed instead of a chat when user don't have any accounts defined [#56633](https://github.com/sourcegraph/sourcegraph/pull/56633)

### Changed

- Improved settings UI [#55876](https://github.com/sourcegraph/sourcegraph/pull/55876)
- Telemetry and other GraphQL requests are now sent through the agent [56001](https://github.com/sourcegraph/sourcegraph/pull/56001)
- Use agent for recipes [#56196](https://github.com/sourcegraph/sourcegraph/pull/56196)
- Authentication settings changed to accounts from three types of instance types to which user can connect [#56362](https://github.com/sourcegraph/sourcegraph/pull/56362)
- Enabled manually triggering autocomplete even when implicit autocomplete is disabled (globally or for a language) [#56473](https://github.com/sourcegraph/sourcegraph/pull/56473)
- Onboarding notifications merged into one simple notification to Open Cody [#56610](https://github.com/sourcegraph/sourcegraph/pull/56610)
- Enabled implicit autocomplete by default [#56617](https://github.com/sourcegraph/sourcegraph/pull/56617)
- Bumped JetBrains platform plugin compat to `221.5080.210` and higher [#56625](https://github.com/sourcegraph/sourcegraph/pull/56625)

### Deprecated

### Removed

- All network traffic from the plugin process [56001](https://github.com/sourcegraph/sourcegraph/pull/56001)
- Non-agent autocomplete and chat [55997](https://github.com/sourcegraph/sourcegraph/pull/55997)
- Support for 2022.0, 2022.1 is now required [#55831](https://github.com/sourcegraph/sourcegraph/pull/55831)
- Removed code search onboarding notification [#56564](https://github.com/sourcegraph/sourcegraph/pull/56564)

### Fixed

- Removing autocomplete inlays when ESC key is pressed when using Cody alongside the IdeaVIM plugin [#56347](https://github.com/sourcegraph/sourcegraph/pull/56347)
- Handle uncaught exception [#56048](https://github.com/sourcegraph/sourcegraph/pull/56048)
- Start the agent process on Windows [#56055](https://github.com/sourcegraph/sourcegraph/pull/56055)
- Internal: use `Autocomplete` instead of `AutoComplete` [#56106](https://github.com/sourcegraph/sourcegraph/pull/56106)
- Start the agent process on installation events [#56116](https://github.com/sourcegraph/sourcegraph/pull/56116)
- Cancel outdated autocomplete requests [#56119](https://github.com/sourcegraph/sourcegraph/pull/56119) [sourcegraph/cody#787](https://github.com/sourcegraph/cody/pull/787)
- Await on agent server before submitting telemetry events [#56007](https://github.com/sourcegraph/sourcegraph/pull/56007)
- Bug causing exceptions to get thrown on editor events [#55999](https://github.com/sourcegraph/sourcegraph/pull/55999)
- Use inferred codebase for autocomplete [#55900](https://github.com/sourcegraph/sourcegraph/pull/55900)
- Make sure caret is visible after accepting multiline completion [#55924](https://github.com/sourcegraph/sourcegraph/pull/55924)
- Suppress duplicate telemetry when using agent [cody#689](https://github.com/sourcegraph/cody/pull/689)
- Fixed bug causing the agent to not work [#55867](https://github.com/sourcegraph/sourcegraph/pull/55867)
- Fixed `NullPointerException` bug [#55869](https://github.com/sourcegraph/sourcegraph/pull/55869)
- Update telemetry to include whether other completion plugins are installed [#55932](https://github.com/sourcegraph/sourcegraph/pull/55932)

### Security

## [3.0.9]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [3.0.9]

### Changed

- Store application level access tokens in a safe way [#55251](https://github.com/sourcegraph/sourcegraph/pull/55251)
- Autocomplete is now powered by the agent when enabled (off by default) [#55638](https://github.com/sourcegraph/sourcegraph/pull/55638), [#55826](https://github.com/sourcegraph/sourcegraph/pull/55826)

### Fixed

- Removed jumping text effect from the chat when generating response [#55357](https://github.com/sourcegraph/sourcegraph/pull/55357)
- Chat message doesn't jump after finished response generation [#55390](https://github.com/sourcegraph/sourcegraph/pull/55390)
- Removed jumping text effect from the chat when generating response [#55357](https://github.com/sourcegraph/sourcegraph/pull/55357)

## [3.0.8]

### Fixed

- Improved the auto-scrolling of the Cody chat [#55150](https://github.com/sourcegraph/sourcegraph/pull/55150)
- Fixed mouse wheel and mouse drag scrolling in the Cody chat [#55199](https://github.com/sourcegraph/sourcegraph/pull/55199)

## [3.0.7]

### Added

- New menu item in the toolbar cogwheel menu to open the Cody app settings [#55146](https://github.com/sourcegraph/sourcegraph/pull/55146)

### Changed

- Improved UI of the onboarding widgets [#55090](https://github.com/sourcegraph/sourcegraph/pull/55090)
- Improved perceived autocomplete performance [#55098](https://github.com/sourcegraph/sourcegraph/pull/55098)

### Fixed

- Enable/disable Cody automatically based on the settings [#55138](https://github.com/sourcegraph/sourcegraph/pull/55138)

## [3.0.6]

### Added

- Automatic detection of Cody app status in the settings window [#54955](https://github.com/sourcegraph/sourcegraph/pull/54955)
- Add "Enable Cody" option to settings [#55004](https://github.com/sourcegraph/sourcegraph/pull/55004)

### Changed

- Disable "summarize recent code changes" button if git repository is not available [#54859](https://github.com/sourcegraph/sourcegraph/pull/54859)
- Get the chat model max tokens value from the instance when available [#54954](https://github.com/sourcegraph/sourcegraph/pull/54954)

### Fixed

- Downgraded connection errors for invalid or inaccessible enterprise instances to warnings [#54916](https://github.com/sourcegraph/sourcegraph/pull/54916)
- Try to log error stack traces and recover from them, rather than re-throw the exception [#54917](https://github.com/sourcegraph/sourcegraph/pull/54917)
- Show only one informative message in case of invalid access token [#54951](https://github.com/sourcegraph/sourcegraph/pull/54951)
- Don't display `<br />` tag in the chat message when trying to insert new line in the code block [#55007](https://github.com/sourcegraph/sourcegraph/pull/55007)

## [3.0.5]

### Added

- Added embeddings status in footer [#54575](https://github.com/sourcegraph/sourcegraph/pull/54575)
- Added currently opened file name in footer [#54610](https://github.com/sourcegraph/sourcegraph/pull/54610)
- Auto-growing prompt input [#53594](https://github.com/sourcegraph/sourcegraph/pull/53594)
- Added "stop generating" button [#54710](https://github.com/sourcegraph/sourcegraph/pull/54710)
- Copy code block button added to editor in the chat message to copy the text to clipboard [#54783](https://github.com/sourcegraph/sourcegraph/pull/54783)
- Insert at Cursor button added to editor in the chat message to insert the text form the editor to main editor [#54815](https://github.com/sourcegraph/sourcegraph/pull/54815)
- Added support for multiline autocomplete [#54848](https://github.com/sourcegraph/sourcegraph/pull/54848)

### Fixed

- Fixed telemetry for Sourcegraph.com [#54885](https://github.com/sourcegraph/sourcegraph/pull/54885)

## [3.0.4]

### Added

- Added embeddings status in footer [#54575](https://github.com/sourcegraph/sourcegraph/pull/54575)
- Added currently opened file name in footer [#54610](https://github.com/sourcegraph/sourcegraph/pull/54610)
- Added "stop generating" button [#54710](https://github.com/sourcegraph/sourcegraph/pull/54710)
- Made prompt input grow automatically [#53594](https://github.com/sourcegraph/sourcegraph/pull/53594)

### Changed

- Fixed logging to use JetBrains api + other minor fixes [#54579](https://github.com/sourcegraph/sourcegraph/pull/54579)
- Enabled editor recipe context menu items when working with Cody app only when Cody app is running [#54583](https://github.com/sourcegraph/sourcegraph/pull/54583)
- Renamed `completion` to `autocomplete` in both the UI and code [#54606](https://github.com/sourcegraph/sourcegraph/pull/54606)
- Increased minimum rows of prompt input form 2 to 3 [#54733](https://github.com/sourcegraph/sourcegraph/pull/54733)
- Improved completion prompt with changes from the VS Code plugin [#54668](https://github.com/sourcegraph/sourcegraph/pull/54668)
- Displayed more informative message when no context has been found [#54480](https://github.com/sourcegraph/sourcegraph/pull/54480)

### Fixed

- Now avoiding NullPointerException in an edge case when the chat doesn't exist [#54785](https://github.com/sourcegraph/sourcegraph/pull/54785)

## [3.0.3]

### Added

- Added recipes to editor context menu [#54430](https://github.com/sourcegraph/sourcegraph/pull/54430)
- Figure out default repository when no files are opened in the editor [#54476](https://github.com/sourcegraph/sourcegraph/pull/54476)
- Added `unstable-codegen` completions support [#54435](https://github.com/sourcegraph/sourcegraph/pull/54435)

### Changed

- Use smaller Cody logo in toolbar and editor context menu [#54481](https://github.com/sourcegraph/sourcegraph/pull/54481)
- Sourcegraph link sharing and opening file in browser actions are disabled when working with Cody app [#54473](https://github.com/sourcegraph/sourcegraph/pull/54473)

### Fixed

- Preserve new lines in the human chat message [#54417](https://github.com/sourcegraph/sourcegraph/pull/54417)
- JetBrains: Handle response == null case when checking for embeddings [#54492](https://github.com/sourcegraph/sourcegraph/pull/54492)

## [3.0.2]

### Fixed

- Repositories with http/https remotes are now available for Cody [#54372](https://github.com/sourcegraph/sourcegraph/pull/54372)

## [3.0.1]

### Changed

- Sending message on Enter rather than Ctrl/Cmd+Enter [#54331](https://github.com/sourcegraph/sourcegraph/pull/54331)
- Updated name to Cody AI app [#54360](https://github.com/sourcegraph/sourcegraph/pull/54360)

### Removed

- Sourcegraph CLI's SRC_ENDPOINT and SRC_ACCESS_TOKEN env variables overrides for the local config got removed [#54369](https://github.com/sourcegraph/sourcegraph/pull/54369)

### Fixed

- telemetry is now being sent to both the current instance & dotcom (unless the current instance is dotcom, then just that) [#54347](https://github.com/sourcegraph/sourcegraph/pull/54347)
- Don't display doubled messages about the error when trying to load context [#54345](https://github.com/sourcegraph/sourcegraph/pull/54345)
- Now handling Null error messages in error logging properly [#54351](https://github.com/sourcegraph/sourcegraph/pull/54351)
- Made sidebar refresh work for non-internal builds [#54348](https://github.com/sourcegraph/sourcegraph/pull/54358)
- Don't display duplicated files in the "Read" section in the chat [#54363](https://github.com/sourcegraph/sourcegraph/pull/54363)
- Repositories without configured git remotes are now available for Cody [#54370](https://github.com/sourcegraph/sourcegraph/pull/54370)
- Repositories with http/https remotes are now available for Cody [#54372](https://github.com/sourcegraph/sourcegraph/pull/54372)

## [3.0.0]

### Added

- Background color and font of inline code blocks differs from regular text in message [#53761](https://github.com/sourcegraph/sourcegraph/pull/53761)
- Autofocus Cody chat prompt input [#53836](https://github.com/sourcegraph/sourcegraph/pull/53836)
- Basic integration with the local Cody App [#54061](https://github.com/sourcegraph/sourcegraph/pull/54061)
- Background color and font of inline code blocks differs from regular text in message [#53761](https://github.com/sourcegraph/sourcegraph/pull/53761)
- Autofocus Cody chat prompt input [#53836](https://github.com/sourcegraph/sourcegraph/pull/53836)
- Cody Agent [#53370](https://github.com/sourcegraph/sourcegraph/pull/53370)
- Chat message when access token is invalid or not
  configured [#53659](https://github.com/sourcegraph/sourcegraph/pull/53659)
- A separate setting for the (optional) dotcom access
  token. [pull/53018](https://github.com/sourcegraph/sourcegraph/pull/53018)
- Enabled "Explain selected code (detailed)"
  recipe [#53080](https://github.com/sourcegraph/sourcegraph/pull/53080)
- Enabled multiple recipes [#53299](https://github.com/sourcegraph/sourcegraph/pull/53299)
  - Explain selected code (high level)
  - Generate a unit test
  - Generate a docstring
  - Improve variable names
  - Smell code
  - Optimize code
- A separate setting for enabling/disabling Cody completions. [pull/53302](https://github.com/sourcegraph/sourcegraph/pull/53302)
- Debounce for inline Cody completions [pull/53447](https://github.com/sourcegraph/sourcegraph/pull/53447)
- Enabled "Translate to different language" recipe [#53393](https://github.com/sourcegraph/sourcegraph/pull/53393)
- Skip Cody completions if there is code in line suffix or in the middle of a word in prefix [#53476](https://github.com/sourcegraph/sourcegraph/pull/53476)
- Enabled "Summarize recent code changes" recipe [#53534](https://github.com/sourcegraph/sourcegraph/pull/53534)

### Changed

- Convert `\t` to spaces in leading whitespace for autocomplete suggestions (according to settings) [#53743](https://github.com/sourcegraph/sourcegraph/pull/53743)
- Disabled line highlighting in code blocks in chat [#53829](https://github.com/sourcegraph/sourcegraph/pull/53829)
- Parallelized completion API calls and reduced debounce down to 20ms [#53592](https://github.com/sourcegraph/sourcegraph/pull/53592)

### Fixed

- Fixed the y position at which autocomplete suggestions are rendered [#53677](https://github.com/sourcegraph/sourcegraph/pull/53677)
- Fixed rendered completions being cleared after disabling them in settings [#53758](https://github.com/sourcegraph/sourcegraph/pull/53758)
- Wrap long words in the chat message [#54244](https://github.com/sourcegraph/sourcegraph/pull/54244)
- Reset conversation button re-enables "Send"
  button [#53669](https://github.com/sourcegraph/sourcegraph/pull/53669)
- Fixed font on the chat ui [#53540](https://github.com/sourcegraph/sourcegraph/pull/53540)
- Fixed line breaks in the chat ui [#53543](https://github.com/sourcegraph/sourcegraph/pull/53543)
- Reset prompt input on message send [#53543](https://github.com/sourcegraph/sourcegraph/pull/53543)
- Fixed UI of the prompt input [#53548](https://github.com/sourcegraph/sourcegraph/pull/53548)
- Fixed zero-width spaces popping up in inline autocomplete [#53599](https://github.com/sourcegraph/sourcegraph/pull/53599)
- Reset conversation button re-enables "Send" button [#53669](https://github.com/sourcegraph/sourcegraph/pull/53669)
- Fixed displaying message about invalid access token on any 401 error from backend [#53674](https://github.com/sourcegraph/sourcegraph/pull/53674)

## [3.0.0-alpha.9]

### Added

- Background color and font of inline code blocks differs from regular text in message [#53761](https://github.com/sourcegraph/sourcegraph/pull/53761)
- Autofocus Cody chat prompt input [#53836](https://github.com/sourcegraph/sourcegraph/pull/53836)
- Basic integration with the local Cody App [#54061](https://github.com/sourcegraph/sourcegraph/pull/54061)
- Onboarding of the user when using local Cody App [#54298](https://github.com/sourcegraph/sourcegraph/pull/54298)

## [3.0.0-alpha.7]

### Added

- Background color and font of inline code blocks differs from regular text in message [#53761](https://github.com/sourcegraph/sourcegraph/pull/53761)
- Autofocus Cody chat prompt input [#53836](https://github.com/sourcegraph/sourcegraph/pull/53836)
- Cody Agent [#53370](https://github.com/sourcegraph/sourcegraph/pull/53370)

### Changed

- Convert `\t` to spaces in leading whitespace for autocomplete suggestions (according to settings) [#53743](https://github.com/sourcegraph/sourcegraph/pull/53743)
- Disabled line highlighting in code blocks in chat [#53829](https://github.com/sourcegraph/sourcegraph/pull/53829)

### Fixed

- Fixed the y position at which autocomplete suggestions are rendered [#53677](https://github.com/sourcegraph/sourcegraph/pull/53677)
- Fixed rendered completions being cleared after disabling them in settings [#53758](https://github.com/sourcegraph/sourcegraph/pull/53758)
- Wrap long words in the chat message [#54244](https://github.com/sourcegraph/sourcegraph/pull/54244)
- Reset conversation button re-enables "Send"
  button [#53669](https://github.com/sourcegraph/sourcegraph/pull/53669)

## [3.0.0-alpha.6]

### Added

- Chat message when access token is invalid or not
  configured [#53659](https://github.com/sourcegraph/sourcegraph/pull/53659)

## [3.0.0-alpha.5]

### Added

- A separate setting for the (optional) dotcom access
  token. [pull/53018](https://github.com/sourcegraph/sourcegraph/pull/53018)
- Enabled "Explain selected code (detailed)"
  recipe [#53080](https://github.com/sourcegraph/sourcegraph/pull/53080)
- Enabled multiple recipes [#53299](https://github.com/sourcegraph/sourcegraph/pull/53299)
  - Explain selected code (high level)
  - Generate a unit test
  - Generate a docstring
  - Improve variable names
  - Smell code
  - Optimize code
- A separate setting for enabling/disabling Cody completions. [pull/53302](https://github.com/sourcegraph/sourcegraph/pull/53302)
- Debounce for inline Cody completions [pull/53447](https://github.com/sourcegraph/sourcegraph/pull/53447)
- Enabled "Translate to different language" recipe [#53393](https://github.com/sourcegraph/sourcegraph/pull/53393)
- Skip Cody completions if there is code in line suffix or in the middle of a word in prefix [#53476](https://github.com/sourcegraph/sourcegraph/pull/53476)
- Enabled "Summarize recent code changes" recipe [#53534](https://github.com/sourcegraph/sourcegraph/pull/53534)

### Changed

- Parallelized completion API calls and reduced debounce down to 20ms [#53592](https://github.com/sourcegraph/sourcegraph/pull/53592)

### Fixed

- Fixed font on the chat ui [#53540](https://github.com/sourcegraph/sourcegraph/pull/53540)
- Fixed line breaks in the chat ui [#53543](https://github.com/sourcegraph/sourcegraph/pull/53543)
- Reset prompt input on message send [#53543](https://github.com/sourcegraph/sourcegraph/pull/53543)
- Fixed UI of the prompt input [#53548](https://github.com/sourcegraph/sourcegraph/pull/53548)
- Fixed zero-width spaces popping up in inline autocomplete [#53599](https://github.com/sourcegraph/sourcegraph/pull/53599)
- Reset conversation button re-enables "Send" button [#53669](https://github.com/sourcegraph/sourcegraph/pull/53669)
- Fixed displaying message about invalid access token on any 401 error from backend [#53674](https://github.com/sourcegraph/sourcegraph/pull/53674)

## [3.0.0-alpha.1]

### Added

- Alpha-quality Cody chat, not ready yet for internal dogfooding.
- Alpha-quality Cody code completions, not ready yet for internal dogfooding.

## [2.1.4]

### Added

- Add `extensionDetails` to `public_argument` on logger [#51321](https://github.com/sourcegraph/sourcegraph/pull/51321)

### Fixed

- Handle case when remote for local branch != sourcegraph remote [#52172](https://github.com/sourcegraph/sourcegraph/pull/52172)

## [2.1.3]

### Added

- Compatibility with IntelliJ 2023.1

### Fixed

- Fixed a backward-compatibility issue with Sourcegraph versions prior to 4.3 [#50080](https://github.com/sourcegraph/sourcegraph/issues/50080)

## [2.1.2]

### Added

- Compatibility with IntelliJ 2022.3

## [2.1.1]

### Added

- Now the name of the remote can contain slashes

### Fixed

- “Open in Browser” and “Copy Link” features now open the correct branch when it exists on the remote. [pull/44739](https://github.com/sourcegraph/sourcegraph/pull/44739)
- Fixed a bug where if the tracked branch had a different name from the local branch, the local branch name was used in the URL, incorrectly

## [2.1.0]

### Added

- Perforce support [pull/43807](https://github.com/sourcegraph/sourcegraph/pull/43807)
- Multi-repo project support [pull/43807](https://github.com/sourcegraph/sourcegraph/pull/43807)

### Changed

- Now using the VCS API bundled with the IDE rather than relying on the `git`
  command [pull/43807](https://github.com/sourcegraph/sourcegraph/pull/43807)

## [2.0.2]

### Added

- Added feature to specify auth headers [pull/42692](https://github.com/sourcegraph/sourcegraph/pull/42692)

### Removed

- Removed tracking parameters from all shareable URLs [pull/42022](https://github.com/sourcegraph/sourcegraph/pull/42022)

### Fixed

- Remove pointer cursor in the web view. [pull/41845](https://github.com/sourcegraph/sourcegraph/pull/41845)
- Updated “Learn more” URL to link the blog post in the update notification [pull/41846](https://github.com/sourcegraph/sourcegraph/pull/41846)
- Made the plugin compatible with versions 3.42.0 and below [pull/42105](https://github.com/sourcegraph/sourcegraph/pull/42105)

## [2.0.1]

- Improve Fedora Linux compatibility: Using `BrowserUtil.browse()` rather than `Desktop.getDesktop().browse()` to open
  links in the browser.

## [2.0.0]

- Added a new UI to search with Sourcegraph from inside the IDE. Open it with <kbd>Alt+S</kbd> (<kbd>⌥S</kbd> on Mac) by
  default.
- Added a settings UI to conveniently configure the plugin
- General revamp on the existing features
- Source code is now
  at [https://github.com/sourcegraph/sourcegraph/tree/main/client/jetbrains](https://github.com/sourcegraph/sourcegraph/tree/main/client/jetbrains)

## [1.2.4]

- Fixed an issue that prevent the latest version of the plugin to work with JetBrains 2022.1 products.

## [1.2.3]

- Upgrade JetBrains IntelliJ shell to 1.3.1 and modernize the build and release pipeline.

## [1.2.2] - Minor bug fixes

- It is now possible to configure the plugin per-repository using a `.idea/sourcegraph.xml` file. See the README for details.
- Special thanks: @oliviernotteghem for contributing the new features in this release!
- Fixed bugs where Open in Sourcegraph from the git menu does not work for repos with ssh url as their remote url

## [1.2.1] - Open Revision in Sourcegraph

- Added "Open In Sourcegraph" action to VCS History and Git Log to open a revision in the Sourcegraph diff view.
- Added "defaultBranch" configuration option that allows opening files in a specific branch on Sourcegraph.
- Added "remoteUrlReplacements" configuration option that allow users to replace specified values in the remote url with new strings.

## [1.2.0] - Copy link to file, search in repository, per-repository configuration, bug fixes & more

- The search menu entry is now no longer present when no text has been selected.
- When on a branch that does not exist remotely, `master` will now be used instead.
- Menu entries (Open file, etc.) are now under a Sourcegraph sub-menu.
- Added a "Copy link to file" action (alt+c / opt+c).
- Added a "Search in repository" action (alt+r / opt+r).
- It is now possible to configure the plugin per-repository using a `.idea/sourcegraph.xml` file. See the README for details.
- Special thanks: @oliviernotteghem for contributing the new features in this release!

## [1.1.2] - Minor bug fixes around searching.

- Fixed an error that occurred when trying to search with no selection.
- The git remote used for repository detection is now `sourcegraph` and then `origin`, instead of the previously poor choice of just the first git remote.

## [1.1.1] - Fixed search shortcut

- Updated the search URL to reflect a recent Sourcegraph.com change.

## [1.1.0] - Configurable Sourcegraph URL

- Added support for using the plugin with on-premises Sourcegraph instances.

## [1.0.0] - Initial Release

- Basic Open File & Search functionality.
