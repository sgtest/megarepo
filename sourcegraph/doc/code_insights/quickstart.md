# Quickstart for Code Insights

Get started and create your first [code insight](index.md) in 5 minutes or less.

## Introduction

In this guide, you'll create a Sourcegraph code insight that tracks the number of `TODOS` that appear in parts of your codebase. 

For more information about Code Insights see the [Code Insights](index.md) documentation. 

<img src="https://sourcegraphstatic.com/docs/images/code_insights/quickstart_TODOs_insight_dark.png" class="screenshot">

## Requirements

- You are a Sourcegraph enterprise customer. (Want code insights but aren't enterprise? [Let us know](mailto:feedback@sourcegraph.com).)
- Your Sourcegraph instance has at least 1 repository. (See "[Quickstart](../../index.md#quickstart)" on how to setup a Sourcegraph instance.)
- You are running Sourcegraph version 3.28 (May 2021 release) or later.
    - Note: If you're on Sourcegraph version 3.24 or later, you can instead follow [this gist](https://gist.github.com/Joelkw/f0582b164578aabc3ac936dee43f23e0) to create an insight. Due to the early stage of the product, it's more likely you'll run into trouble, though, so we recommend that you either upgrade your Sourcegraph or reach out to your Sourcegraph reps for help.

## Enable Code Insights

### 1. Enable the experimental feature flag

Add the following to either your Sourcegraph user settings `sourcegraph.example.com/users/[username]/settings` or organization settings `sourcegraph.example.com/organizations/[your_org]/settings`: 

`"experimentalFeatures": { "codeInsights": true }`

If you put this flag in your organization settings, everyone on your Sourcegraph insights will be able to see the "Insights" navbar menu item and create their own code insights. If you put the flag in your user settings, only you will have those abilities. 

(Enabling code insights organization-wide doesn't mean that other users can automatically see the code insights you create, however – you can control that visibility per individual insight.)

### 2. Visit your sourcegraph.example.com/insights page and select "+ Create new insight" 

### 3. On the insight type selection page, select "Create custom insight"

This creates a code insight tracking an arbitrary input that you could run a Sourcegraph search for. 

If you are more interested in creating a language-based insight to show you language breakdown in your repositories, [follow this tutorial](language_insight_quickstart.md) instead. 

### 4. Once on the "Create New Code Insight" form fields page, enter the repositories you want to search

Enter repositories in the repository URL format, like `github.com/Sourcegraph/Sourcegraph`. Separate multiple repositories with a comma. The form field will validate that you've entered the repository correctly. 

### 5. Define a data series to track the incidence of `TODO`

A data series becomes a line on your graph. 

You can **Name** this data series something like `TODOs count`.

To track the incidence of TODOs, you can set your **Search query** to be simply `TODO`. 

You can also select the color of your data series. 

### 6. Add a title to the insight

Enter a descriptive **Title** for the chart, like `Count of TODOs in [repository name]`.

### 7. Set the visibility of your insight

This controls who else can see your insight. 

Anything set to "Personal" won't be visible by anyone else. Otherwise, everyone in the selected organization can see your insight (if they have also enabled [the feature flag](#1-enable-the-experimental-feature-flag)).

### 8. Set the distance between data points to 1 month

The code insights prototypes currently show you seven datapoints for each data series. Your code insight will therefore show you results for a time horizon that is 6 * [distance between datapoints]. Setting it to one month means you'll see the results over the last six months. 

### 9. Click "create code insight" and view your insight. 

You'll be taken to the sourcegraph.example.com/insights page and can view your insight.