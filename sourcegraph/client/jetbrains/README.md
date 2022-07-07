<!-- Plugin description -->

# Sourcegraph for JetBrains IDEs

- **New:** Search with Sourcegraph directly from inside your JetBrains product.
- Instantly search in all open source repos and your private code.
- Peek into any remote repo in the IDE, without checking it out.
- Create URLs to specific code blocks to share them with your teammates.
- Open your files on Sourcegraph.

<!-- Plugin description end -->

## Supported IDEs [![JetBrains Plugin](https://img.shields.io/badge/JetBrains-Sourcegraph-green.svg)](https://plugins.jetbrains.com/plugin/9682-sourcegraph)

The plugin works with all JetBrains IDEs, including:

- IntelliJ IDEA
- IntelliJ IDEA Community Edition
- PhpStorm
- WebStorm
- PyCharm
- PyCharm Community Edition
- RubyMine
- AppCode
- CLion
- GoLand
- DataGrip
- Rider
- Android Studio

## Installation

- Open settings
  - Mac: Go to `IntelliJ IDEA | Preferences` (or use <kbd>⌘,</kbd>)
  - Windows: Go to `File | Settings` (or use <kb>Ctrl+Alt+S</kb>)
- Click `Plugins` in the left-hand pane, then the `Marketplace` tab at the top
- Search for `Sourcegraph`, then click the `Install` button
- Restart your IDE if needed
- To try the plugin, press <kbd>Alt+A</kbd> (<kbd>⌥A</kbd> on Mac) then select some code and choose `Sourcegraph` in the right-click context menu to see actions and keyboard shortcuts.

## Configuring for use with a private Sourcegraph instance

The plugin is configurable _globally_ by creating a `.sourcegraph-jetbrains.properties` (or `sourcegraph-jetbrains.properties` pre-v1.2.2) in your home directory. For example, modify the following URL to match your on-premises Sourcegraph instance URL:

```
url = https://sourcegraph.example.com
defaultBranch = example-branch
remoteUrlReplacements = git.example.com, git-web.example.com
```

You may also choose to configure it _per project_ using a `.idea/sourcegraph.xml` file in your project like so:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="com.sourcegraph.config.SourcegraphConfig">
    <option name="url" value="https://sourcegraph.example.com"/>
    <option name="defaultBranch" value="example-branch"/>
    <option name="remoteUrlReplacements" value="git.example.com, git-web.example.com"/>
  </component>
</project>
```

By default, the plugin will use the git remote called `origin` to determine which repository on Sourcegraph corresponds to your local repository. If your `origin` remote doesn’t match Sourcegraph, you may instead configure a Git remote by the name of `sourcegraph`, and it will take priority.

## Questions & Feedback

If you have any questions, feedback, or bug report, we appreciate if you [open an issue on GitHub](https://github.com/sourcegraph/sourcegraph/issues/new?title=JetBrains:+&labels=jetbrains-ide).

## Uninstallation

- Open settings
  - Mac: Go to `IntelliJ IDEA | Preferences` (or use <kbd>⌘,</kbd>)
  - Windows: Go to `File | Settings` (or use <kb>Ctrl+Alt+S</kb>)
- Click `Plugins` in the left-hand pane, then the `Installed` tab at the top
- Find `Sourcegraph` → Right click → `Uninstall` (or uncheck to disable)

## Development

- Clone `https://github.com/sourcegraph/sourcegraph`
- Run `yarn install` in the root directory to get all dependencies
- Run `yarn generate` in the root directory to generate graphql files
- Go to `client/jetbrains/` and run `yarn build` to generate the JS files, or `yarn watch` to watch for changes and regenerate on the fly
- You can test the “Find with Sourcegraph” window by running `yarn standalone` in the `client/jetbrains/` directory and opening [http://localhost:3000/](http://localhost:3000/) in your browser.
- Run the plugin in a sandboxed IDE by running `./gradlew runIde`. This will start the platform with the versions defined in `gradle.properties`, [here](https://github.com/sourcegraph/sourcegraph/blob/main/client/jetbrains/gradle.properties#L14-L16).
  - Note: 2021.3 or later is required for Macs with Apple Silicon chips.
- Build a deployable plugin artifact by running `./gradlew buildPlugin`. The output file is `build/distributions/Sourcegraph.zip`.

## Publishing a new version

The publishing process is based on the [intellij-platform-plugin-template](https://github.com/JetBrains/intellij-platform-plugin-template).

### Publishing from your local machine

1. Update `pluginVersion` in `gradle.properties`

- Pre-release builds with the same version as a previous one need to append a number. For example, `1.0.0-alpha`, then `1.0.0-alpha.1`, `1.0.0-alpha.2`, and so on.

2. Describe the changes in the `[Unreleased]` section of `client/jetbrains/CHANGELOG.md`
3. Run `PUBLISH_TOKEN=<YOUR TOKEN HERE> ./scripts/release.sh` from inside the `client/jetbrains` directory (You can [generate tokens on the JetBrains marketplace](https://plugins.jetbrains.com/author/me/tokens)).

## Version History

See [`CHANGELOG.md`](https://github.com/sourcegraph/sourcegraph/blob/main/client/jetbrains/CHANGELOG.md).

## Enable web view debugging

Parts of this extension rely on the [JCEF](https://plugins.jetbrains.com/docs/intellij/jcef.html) web view features built into the JetBrains platform. To enable debugging tools for this view, please follow these steps:

1. [Enable JetBrains internal mode](https://plugins.jetbrains.com/docs/intellij/enabling-internal.html)
2. Open Search Everywhere and the "Actions" tab: (On macOS via `cmd+shift+a`)
3. Search for "Registry..." and open it
4. Search for an option called `ide.browser.jcef.debug.port`
5. Change the default value to an open port (e.g. `9222`)
6. After this, a restart of the IDE may be required
7. Open the Sourcegraph search inside JetBrains `alt+a`
8. Now, you can switch to a browser window, navigate to [`localhost:9222`](http://localhost:9222), and select the Sourcegraph window.
