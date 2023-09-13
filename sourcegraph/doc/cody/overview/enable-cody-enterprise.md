# Enabling Cody on Sourcegraph Enterprise

- [Instructions for self-hosted Sourcegraph Enterprise](#cody-on-self-hosted-sourcegraph-enterprise)
- [Instructions for Sourcegraph Cloud](#cody-on-sourcegraph-cloud)
- [Enabling codebase-aware answers](#enabling-codebase-aware-answers)
- [Turning Cody off](#turning-cody-off)

## Cody on self-hosted Sourcegraph Enterprise

### Prerequisites

- Sourcegraph 5.1.0 or above
- A Sourcegraph Enterprise subscription with [Cody Gateway access](./../explanations/cody_gateway.md), or [an account with a third-party LLM provider](#using-a-third-party-llm-provider-directly).

There are two steps required to enable Cody on your enterprise instance:

1. Enable Cody on your Sourcegraph instance
2. Configure the VS Code extension

### Step 1: Enable Cody on your Sourcegraph instance

> NOTE: Cody uses one or more third-party LLM (Large Language Model) providers. Make sure you review the [Cody usage and privacy notice](https://about.sourcegraph.com/terms/cody-notice). In particular, code snippets will be sent to a third-party language model provider when you use the Cody extension or when embeddings are enabled.

This requires site-admin privileges.

1. First, configure your desired LLM provider:
    > NOTE: If you are a Sourcegraph Cloud customer, skip to (3).

    - Recommended: [Using Sourcegraph Cody Gateway](./../explanations/cody_gateway.md#using-cody-gateway-in-sourcegraph-enterprise)
    - [Using a third-party LLM provider directly](#using-a-third-party-llm-provider-directly)
2. Go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

    ```json
    {
      // [...]
      "cody.enabled": true
    }
    ```
3. Set up a policy to automatically create embeddings for repositories: ["Configuring embeddings"](./../explanations/code_graph_context.md#configuring-embeddings)

Cody is now fully set up on your instance!

### Step 2: Configure the VS Code extension

Now that Cody is turned on on your Sourcegraph instance, any user can configure and use the Cody VS Code extension. This does not require admin privilege.

1. If you currently have a previous version of Cody installed, uninstall it and reload VS Code before proceeding to the next steps.
2. Search for "Cody AI” in your VS Code extension marketplace, and install it.

  <img width="500" alt="Sourcegraph Cody in VS Code Marketplace" src="https://storage.googleapis.com/sourcegraph-assets/cody-in-marketplace.png">

3. Reload VS Code, and open the Cody extension. <!-- Review and accept the terms. (this has been removed?) -->

4. Now you'll need to point the Cody extension to your Sourcegraph instance. Click on "Other Sign In Options..." and select your enterpise option depending on your sourcegraph version (to check your Sourcegraph version go to Sourcegraph => Settings and the version will be in the bottom left)

  <img width="1369" alt="image" src="https://storage.googleapis.com/sourcegraph-assets/cody-sign-in-options.png">

5. If you on version 5.1 and above you will just need to follow an authorization flow to give Cody access to your enterpise instance.

    - For Sourcegraph 5.0 and above you'll need to generate an access token. On your Sourcegraph instance, click on **Settings**, then on **Access tokens** (`https://<your-instance>.sourcegraph.com/users/<your-instance>/settings/tokens`). Generate an access token, copy it, and set it in the Cody extension.

    <img width="1369" alt="image" src="https://user-images.githubusercontent.com/25070988/227510686-4afcb1f9-a3a5-495f-b1bf-6d661ba53cce.png">

    - After creating your access token, copy it and return to VS code. Click on the "Other Sign In Options..." button and select "Sign in to Sourcegraph Enterprise instance via Access Token".
    - Enter the URL for your sourcegraph instance and then paste in your access token.

    <!-- <img width="553" alt="image" src="https://user-images.githubusercontent.com/25070988/227510233-5ce37649-6ae3-4470-91d0-71ed6c68b7ef.png"> -->

You're all set!

### Step 3: Try Cody!

These are a few things you can ask Cody:

- "What are popular go libraries for building CLIs?"
- Open your workspace, and ask "Do we have a React date picker component in this repository?"
- Right click on a function, and ask Cody to explain it

[See more Cody use cases here](./../use-cases.md).

## Cody on Sourcegraph Cloud

On [Sourcegraph Cloud](../../cloud/index.md), Cody is a managed service and you do not need to follow step 1 of the self-hosted guide above.

### Step 1: Enable Cody for your instance

Cody can be enabled on demand on your Sourcegraph instance by contacting your account manager. The Sourcegraph team will refer to the [handbook](https://handbook.sourcegraph.com/departments/cloud/#managed-instance-requests).

### Step 2: Configure the VS Code extension
[See above](#step-2-configure-the-vs-code-extension).

### Step 3: Try Cody!
[See above](#step-3-try-cody).

[Learn more about running Cody on Sourcegraph Cloud](../../cloud/index.md#cody).

## Enabling codebase-aware answers

> NOTE: In order to enable codebase-aware answers for Cody, you must first [configure code graph context](./../explanations/code_graph_context.md).

The `Cody: Codebase` setting in VS Code enables codebase-aware answers for the Cody extension. By setting this configuration option to the repository name on your Sourcegraph instance, Cody will be able to provide more accurate and relevant answers to your coding questions, based on the context of the codebase you are currently working in.

1. Open the VS Code workspace settings by pressing <kbd>Cmd/Ctrl+,</kbd>, (or File > Preferences (Settings) on Windows & Linux).
2. Search for the `Cody: Codebase` setting.
3. Enter the repository name as listed on your Sourcegraph instance.
   1. For example: `github.com/sourcegraph/sourcegraph` without the `https` protocol

## Turning Cody off

To turn Cody off:

1. Go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

    ```json
    {
      // [...]
      "cody.enabled": false
    }
    ```
2. Remove `completions` and `embeddings` configuration if they exist.

## Turning Cody on, only for some users

To turn Cody on only for some users, for example when rolling out a Cody POC, follow all the steps in [Step 1: Enable Cody on your Sourcegraph instance](#step-1-enable-cody-on-your-sourcegraph-instance). Then use the feature flag `cody` to turn Cody on selectively for some users.
To do so:

1. Go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

    ```json
    {
      // [...]
      "cody.enabled": true,
      "cody.restrictUsersFeatureFlag": true
    }
    ```
1. Go to **Site admin > Feature flags** (`/site-admin/feature-flags`)
1. Add a feature flag called `cody`. Select the `boolean` type and set it to `false`.
1. Once added, click on the feature flag and use **add overrides** to pick users that will have access to Cody.

<img width="979" alt="Add overides" src="https://user-images.githubusercontent.com/25070988/235454594-9f1a6b27-6882-44d9-be32-258d6c244880.png">

## Using a third-party LLM provider directly

Instead of [Sourcegraph Cody Gateway](./../explanations/cody_gateway.md), you can configure Sourcegraph to use a third-party provider directly. Currently, this can be one of
- Anthropic
- OpenAI
- Azure OpenAI <span class="badge badge-experimental">Experimental</span>

### Anthropic

First, you must create your own key with Anthropic [here](https://console.anthropic.com/account/keys). Once you have the key, go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

```jsonc
{
  // [...]
  "cody.enabled": true,
  "completions": {
    "provider": "anthropic",
    "chatModel": "claude-2", // Or any other model you would like to use
    "fastChatModel": "claude-instant-1", // Or any other model you would like to use
    "completionModel": "claude-instant-1", // Or any other model you would like to use
    "accessToken": "<key>"
  }
}
```

### OpenAI

First, you must create your own key with OpenAI [here](https://beta.openai.com/account/api-keys). Once you have the key, go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

```jsonc
{
  // [...]
  "cody.enabled": true,
  "completions": {
    "provider": "openai",
    "chatModel": "gpt-4", // Or any other model you would like to use
    "fastChatModel": "gpt-35-turbo", // Or any other model you would like to use
    "completionModel": "gpt-35-turbo", // Or any other model you would like to use
    "accessToken": "<key>"
  }
}
```

_[*OpenAI models supported](https://platform.openai.com/docs/models)_

### Azure OpenAI <span class="badge badge-experimental">Experimental</span>

> NOTE: Azure OpenAI support is experimental.

First, make sure you created a project in the Azure OpenAI portal.

From the project overview, go to **Keys and Endpoint** and grab **one of the keys** on that page, and the **endpoint**.

Next, under **Model deployments** click "manage deployments" and make sure you deploy the models you want to use. For example, `gpt-35-turbo`. Take note of the **deployment name**.

Once done, go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

```jsonc
{
  // [...]
  "cody.enabled": true,
  "completions": {
    "provider": "azure-openai",
    "chatModel": "<deployment name of the model>",
    "fastChatModel": "<deployment name of the model>",
    "completionModel": "<deployment name of the model>",
    "endpoint": "<endpoint>",
    "accessToken": "<key>"
  }
}
```

### Anthropic Claude through AWS Bedrock <span class="badge badge-experimental">Experimental</span>

> NOTE: AWS Bedrock support is experimental.

First, make sure you have access to AWS Bedrock (currently in beta). Next, request access to the Anthropic Claude models in Bedrock.
This may take some time to provision.

Next, create an IAM user with programmatic access in your AWS account. Depending on your AWS setup, different ways may be required to provide access. All completions requests are made from the `frontend` service, so this service needs to be able to access AWS. You can either use instance role bindings, or directly configure the IAM user credentials in configuration.

Once ready, go to **Site admin > Site configuration** (`/site-admin/configuration`) on your instance and set:

```jsonc
{
  // [...]
  "cody.enabled": true,
  "completions": {
    "provider": "aws-bedrock",
    "chatModel": "anthropic.claude-v2",
    "fastChatModel": "anthropic.claude-instant-v1",
    "completionModel": "anthropic.claude-instant-v1",
    "endpoint": "<AWS-Region>", // For example: us-west-2.
    "accessToken": "<See below>"
  }
}
```

For the access token, you can either:
- Leave it empty and rely on instance role bindings or other AWS configurations that are present in the `frontend` service.
- Set it to `<ACCESS_KEY_ID>:<SECRET_ACCESS_KEY>` if directly configuring the credentials.
- Set it to `<ACCESS_KEY_ID>:<SECRET_ACCESS_KEY>:<SESSION_TOKEN>` if a session token is also required.

---

Similarly, you can also [use a third-party LLM provider directly for embeddings](./../explanations/code_graph_context.md#using-a-third-party-embeddings-provider-directly).
