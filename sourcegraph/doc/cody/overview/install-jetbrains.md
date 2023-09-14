<style>

  .markdown-body .cards {
  display: flex;
  align-items: stretch;
}

.markdown-body .cards .card {
  flex: 1;
  margin: 0.5em;
  color: var(--text-color);
  border-radius: 4px;
  border: 1px solid var(--sidebar-nav-active-bg);
  padding: 1.5rem;
  padding-top: 1.25rem;
}

.markdown-body .cards .card:hover {
  color: var(--link-color);
}

.markdown-body .cards .card span {
  color: var(--link-color);
  font-weight: bold;
}

.markdown-body .cards {
  display: flex;
  align-items: stretch;
}

.markdown-body .cards .card {
  flex: 1;
  margin: 0.5em;
  color: var(--text-color);
  border-radius: 4px;
  border: 1px solid var(--sidebar-nav-active-bg);
  padding: 1.5rem;
  padding-top: 1.25rem;
}

.markdown-body .cards .card:hover {
  color: var(--link-color);
}

.markdown-body .cards .card span {
  color: var(--link-color);
  font-weight: bold;
}

.limg {
  list-style: none;
  margin: 3rem 0 !important;
  padding: 0 !important;
}
.limg li {
  margin-bottom: 1rem;
  padding: 0 !important;
}

.limg li:last {
  margin-bottom: 0;
}

.limg a {
    display: flex;
    flex-direction: column;
    transition-property: all;
   transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1);
     transition-duration: 350ms;
     border-radius: 0.75rem;
  padding-top: 1rem;
  padding-bottom: 1rem;

}

.limg a {
  padding-left: 1rem;
  padding-right: 1rem;
  background: rgb(113 220 232 / 19%);
}

.limg p {
  margin: 0rem;
}
.limg a img {
  width: 1rem;
}

.limg h3 {
  display:flex;
  gap: 0.6rem;
  margin-top: 0;
  margin-bottom: .25rem

}

</style>

# Install Cody for JetBrains <span class="badge badge-experimental" style="margin-left: 0.5rem; vertical-align:middle;">Experimental</span>

<p class="subtitle">Learn how to use Cody and its features with the JetBrains IntelliJ editor.</p>

The Cody extension by Sourcegraph enhances your coding experience in your IDE by providing intelligent code suggestions, context-aware completions, and advanced code analysis. This guide will walk you through the steps to install and set up the Cody within your JetBrains environment.

<ul class="limg">
  <li>
    <a class="card text-left" target="_blank" href="https://plugins.jetbrains.com/plugin/9682-cody-ai-by-sourcegraph">
      <h3><img alt="JetBrains" src="https://storage.googleapis.com/sourcegraph-assets/docs/images/cody/jb_beam.svg" />JetBrains Extension (experimental)</h3>
      <p>Install Cody's free and open source extension for JetBrains.</p>
    </a>
  </li>
  </ul>

## Prerequisites

- You have the latest version of <a href="https://www.jetbrains.com/idea/" target="_blank">JetBrains IntelliJ</a> installed
- You have enabled an instance for [Cody from your Sourcegraph.com](cody-with-sourcegraph.md) account

## Install the JetBrains IntelliJ Cody extension

Follow these steps to install the Cody extension for JetBrains IntelliJ:

- Open JetBrains IntelliJ editor on your local machine
- Open "Settings" (Mac: `⌘+,` Windows: `Ctrl+Alt+S`) and select "Plugins"
- Type and search "Cody AI by Sourcegraph" extension and click "Install"

Alternatively, you can also [Download and install the extension from the Jetbrains marketplace](https://plugins.jetbrains.com/plugin/9682-sourcegraph).

> NOTE: Cody works well equally on other JetBrains IDEs like PyCharm, RubyMine, WebStorm etc. The installation steps remain the same.

## Connect the extension to Sourcegraph

After a successful installation, Cody's icon appears in the side bar. When you click it, you're asked to configure and add your Sourcegraph Access Token that helps you connect to a Sourcegraph instance (either an enterprise instance or Sourcegraph.com).

### For Sourcegraph enterprise users

Log in to your Sourcegraph instance and go to `settings` / `access token` (`https://<your-instance>.sourcegraph.com/users/<your-instance>/settings/tokens`). From here, generate a new access token.

Then, you select the option to `Use an enterprise instance` and you will paste your access token and instance URL address in to the Cody extension.

### For Sourcegraph.com users

Click `Continue with Sourcegraph.com` in the Cody extension. From there, you'll be taken to Sourcegraph.com, which will authenticate your extension.

## Verifying the installation

Once connected, click the Cody icon from the sidebar again, and a panel will open. To verify that the Cody extension has been successfully installed and is working as expected:

- Open a file in a supported programming language like JavaScript, Python, Go, etc.
- As you start typing, Cody should begin providing intelligent suggestions and context-aware completions based on your coding patterns and the context of your code

## Commands

The Cody JetBrains IntelliJ extension also supports pre-built reusable prompts called "Commands" that help you quickly get started with common programming tasks like:

- Explain code
- Generate unit test
- Generate docstring
- Improve variable names
- Translate to different language
- Summarize recent code changes
- Detect code smells
- Generate release notes

## Enable code graph context for context-aware answers (Optional)

You can optionally configure code graph content, which gives Cody the ability to provide context-aware answers. For example, Cody can write example API calls if has context of a codebase's API schema.

Learn more about how to:

- [Configure code graph context for Sourcegraph.com](cody-with-sourcegraph.md#configure-code-graph-context-for-code-aware-answers)
- [Configure code graph context for Sourcegraph Enterprise](enable-cody-enterprise.md#enabling-codebase-aware-answers)

## Updating the extension

JetBrains IntelliJ will typically notify you when updates are available for installed extensions. Follow the prompts to update the Cody extension to the latest version.

## More benefits

Read more about [Cody Capabilities](./../capabilities.md) to learn about all the features it provides to boost your development productivity.

## More resources

For more information on what to do next, we recommend the following resources:

<div class="cards">
  <a class="card text-left" href="./../quickstart"><b>Cody Quickstart</b><p>This guide recommends how to use Cody once you have installed the extension in your VS Code editor.</p></a>
  <a class="card text-left" href="./../use-cases"><b>Cody Use Cases</b><p>Explore some of the most common use cases of Cody that helps you with your development workflow.</p></a>
</div>
