# User authentication (SSO)

> NOTE: User authentication is part of the [**critical configuration**](../config/critical_config.md), which means it must be edited from the [management console](../management_console.md).

Sourcegraph supports the following ways for users to sign in:

- [Builtin](#builtin-password-authentication)
- [GitHub OAuth](#github)
- [GitLab OAuth](#gitlab)
- [OpenID Connect](#openid-connect) (including [Google accounts on G Suite](#g-suite-google-accounts))
- [SAML](#saml)
- [HTTP authentication proxies](#http-authentication-proxies)

The authentication provider is configured in the [`auth.providers`](../config/critical_config.md#authentication-providers) critical configuration option.

### Guidance

If you are unsure which auth provider is right for you, we recommend applying the following rules in
order:

- If you have no external identity providers (i.e., not SSO) or are just trying to spin Sourcegraph
  up as quickly as possible to try, use [`builtin`](#builtin-password-authentication) authentication. You can
  always change the auth configuration later, and user identities from external providers will be
  linked automatically to existing Sourcegraph accounts using verified email addresses.
- If you are deploying Sourcegraph behind a HTTP authentication proxy service, use the
  [`http-header`](#http-authentication-proxies) provider type. The proxy service should handle
  authentication and session management and, in turn, set a HTTP header that indicates the user
  identity to Sourcegraph.
- If you are configuring Sourcegraph to index a GitHub or GitLab instance, we recommend using the
  OAuth provider for that code host. This applies even if the code host itself uses an external
  identity provider (e.g., SAML, OpenID Connect, LDAP, etc.). Sourcegraph will redirect to your code
  host on sign-in and the code host will perform the required sign-in flow before redirecting to
  Sourcegraph on success.
- If you are using an identity provider that supports SAML, use the [SAML auth provider](#saml).
- If you are using an identity provider that supports OpenID Connect (including Google accounts),
  use the [OpenID Connect provider](#openid-connect).
- If you wish to use LDAP and cannot use the GitHub/GitLab OAuth provider as described above, or if
  you wish to use another authentication mechanism that is not yet supported, please [contact
  us](https://github.com/sourcegraph/sourcegraph/issues/new?template=feature_request.md) (we respond
  promptly).

Most users will use only one auth provider, but you can use multiple auth providers if desired to
enable sign-in via multiple services. Identities from different providers will be mapped to a
Sourcegraph user by comparing the user's verified email address to the email address from the
external identity provider.

## Builtin password authentication

The [`builtin` auth provider](../config/critical_config.md#builtin-password-authentication) manages user accounts internally in its own database. It supports user signup, login, and password reset (via email if configured, or else via a site admin).

Site configuration example:

```json
{
  // ...,
  "auth.providers": [{ "type": "builtin", "allowSignup": true }]
}
```

The top-level `auth.public` [critical configuration](../config/critical_config.md) option (default `false`) controls whether anonymous users are allowed to access and use the site without being signed in.

## GitHub

> Note: GitHub authentication is currently beta.

[Create a GitHub OAuth
application](https://developer.github.com/apps/building-oauth-apps/creating-an-oauth-app/) (if using
GitHub Enterprise, create one on your instance, not GitHub.com). Set the following values, replacing
`sourcegraph.example.com` with the IP or hostname of your Sourcegraph instance:

- Homepage URL: `https://sourcegraph.example.com`
- Authorization callback URL: `https://sourcegraph.example.com/.auth/github/callback`

> Note: If you want to enable repository permissions, you should grant your OAuth app permission to
> your GitHub organization(s). You can do that either by creating the OAuth app under your GitHub
> organization (rather than your personal account) or by [following these
> instructions](https://help.github.com/articles/approving-oauth-apps-for-your-organization/).

Then add the following lines to your critical configuration:

```json
{
  // ...
  "auth.providers": [
    {
      "type": "github",
      "url": "https://github.example.com",  // URL of your GitHub instance; can leave empty for github.com
      "displayName": "GitHub",
      "clientID": "replace-with-the-oauth-client-id",
      "clientSecret": "replace-with-the-oauth-client-secret",
      "allowSignup": false  // Set to true to enable anyone with a GitHub account to sign up without invitation
    }
  ]
}
```

Replace the `clientID` and `clientSecret` values with the values from your GitHub OAuth app
configuration.

Leave the `url` field empty for GitHub.com.

Set `allowSignup` to `true` to enable anyone with a GitHub account to sign up without invitation
(typically done only for GitHub Enterprise). If `allowSignup` is `false`, a user can sign in through
GitHub only if an account with the same verified email already exists. If none exists, a site admin
must create one explicitly.

Once you've configured GitHub as a sign-on provider, you may also want to [add GitHub repositories to Sourcegraph](../external_service/github.md#repository-syncing).

## GitLab

> Note: GitLab authentication is currently beta.

[Create a GitLab OAuth application](https://docs.gitlab.com/ee/integration/oauth_provider.html). Set
the following values, replacing `sourcegraph.example.com` with the IP or hostname of your
Sourcegraph instance:

- Authorization callback URL: `https://sourcegraph.example.com/.auth/gitlab/callback`
- Scopes: `api`, `read_user`

```json
{
    // ...
    {
      "type": "gitlab",
      "displayName": "GitLab",
      "clientID": "replace-with-the-oauth-application-id",
      "clientSecret": "replace-with-the-oauth-secret",
      "url": "https://gitlab.example.com"
    }
```

Replace the `clientID` and `clientSecret` values with the values from your GitLab OAuth app
configuration.

Once you've configured GitLab as a sign-on provider, you may also want to [add GitLab repositories
to Sourcegraph](../external_service/gitlab.md#repository-syncing).

## OpenID Connect

The [`openidconnect` auth provider](../config/critical_config.md#openid-connect-including-g-suite) authenticates users via OpenID Connect, which is supported by many external services, including:

- [G Suite (Google accounts)](#g-suite-google-accounts)
- [Okta](https://developer.okta.com/docs/api/resources/oidc.html)
- [Ping Identity](https://www.pingidentity.com/en/resources/client-library/articles/openid-connect.html)
- [Auth0](https://auth0.com/docs/protocols/oidc)
- [Salesforce Identity](https://developer.salesforce.com/page/Inside_OpenID_Connect_on_Force.com)
- [Microsoft Azure Active Directory](https://docs.microsoft.com/en-us/azure/active-directory/develop/active-directory-protocols-openid-connect-code)
- [Google Identity Platform](https://developers.google.com/identity/)
- Known issue: [OneLogin](https://www.onelogin.com/openid-connect) OpenID Connect is not supported (use SAML for OneLogin instead)

To configure Sourcegraph to authenticate users via OpenID Connect:

1.  Create a new OpenID Connect client in the external service (such as one of those listed above).
    - **Redirect/callback URI:** `https://sourcegraph.example.com/.auth/callback` (replace `https://sourcegraph.example.com` with the value of the `externalURL` property in your config)
1.  Provide the OpenID Connect client's issuer, client ID, and client secret in the Sourcegraph critical configuration shown below.
1.  (Optional) Require users to have a specific email domain name to authenticate (e.g., to limit users to only those from your organization).

Example [`openidconnect` auth provider](../config/critical_config.md#openid-connect-including-g-suite) configuration:

```json
{
  // ...
  "externalURL": "https://sourcegraph.example.com",
  "auth.providers": [
    {
      "type": "openidconnect",
      "issuer": "https://oidc.example.com",
      "clientID": "my-client-id",
      "clientSecret": "my-client-secret",
      "requireEmailDomain": "example.com"
    }
  ]
}
```

Sourcegraph supports the OpenID Connect Discovery standard for configuring the auth provider (using the document at, e.g., `https://oidc.example.com/.well-known/openid-configuration`).

See the [`openid` auth provider documentation](../config/critical_config.md#openid-connect-including-g-suite) for the full set of configuration options.

### G Suite (Google accounts)

Google's G Suite supports OpenID Connect, which is the best way to enable Sourcegraph authentication using Google accounts. To set it up:

1.  Create an **OAuth client ID** and client secret in the [Google API credentials console](https://console.developers.google.com/apis/credentials). [Google's interactive OpenID Connect documentation page](https://developers.google.com/identity/protocols/OpenIDConnect#getcredentials):
    - **Application type:** Web application
    - **Name:** Sourcegraph (or any other name your users will recognize)
    - **Authorized JavaScript origins:** (leave blank)
    - **Authorized redirect URIs:** `https://sourcegraph.example.com/.auth/callback` (replace `https://sourcegraph.example.com` with the value of the `externalURL` property in your config)
1.  Use the **client ID** and **client secret** values in Sourcegraph critical configuration (as shown in the example below).
1.  Set your G Suite domain in `requireEmailDomain` to prevent users outside your organization from signing in.

Example [`openidconnect` auth provider](../config/critical_config.md#openid-connect-including-g-suite) configuration for G Suite:

```json
{
  // ...
  "externalURL": "https://sourcegraph.example.com",
  "auth.providers": [
    {
      "type": "openidconnect",
      "issuer": "https://accounts.google.com", // All G Suite domains use this issuer URI.
      "clientID": "my-client-id",
      "clientSecret": "my-client-secret",
      "requireEmailDomain": "example.com"
    }
  ]
}
```

## SAML

The [`saml` auth provider](../config/critical_config.md#saml) authenticates users via SAML 2.0, which is supported by many external services, including:

- [Okta](https://developer.okta.com/standards/SAML/index) - _Admin > Classic UI > Applications > Add Application > Create New App > Web / SAML 2.0_
- [OneLogin](https://www.onelogin.com/saml) - _Administration > Apps > Add Apps > SAML Test Connector (SP)_
- [Ping Identity](https://www.pingidentity.com/en/resources/client-library/articles/saml.html)
- [Auth0](https://auth0.com/docs/protocols/saml)
- [Salesforce Identity](https://help.salesforce.com/articleView?id=sso_saml_setting_up.htm&type=0)
- [Microsoft Azure Active Directory](https://docs.microsoft.com/en-us/azure/active-directory/develop/active-directory-single-sign-on-protocol-reference)

To configure Sourcegraph to authenticate users via SAML:

1.  Register Sourcegraph as a SAML Service Provider in the external SAML Identity Provider (such as one of those listed above). Use the following settings (the exact names and labels vary across services).
    - **Assertion Consumer Service URL, Recipient URL, Destination URL, Single sign-on URL:** `https://sourcegraph.example.com/.auth/saml/acs` (substituting the `externalURL` from your [critical configuration](../config/critical_config.md))
    - **Service Provider (issuer, entity ID, audience URI, metadata URL):** `https://sourcegraph.example.com/.auth/saml/metadata` (substituting the `externalURL` from your [critical configuration](../config/critical_config.md))
    - **Attribute statements:**
      - `email` (required): the user's email
      - `login` (optional): the user's username
      - `displayName` (optional): the full name of the user
1.  Obtain the SAML identity provider metadata URL and use it in Sourcegraph critical configuration as shown below.

Example [`saml` auth provider](../config/critical_config.md#saml) configuration:

```json
{
  // ...
  "externalURL": "https://sourcegraph.example.com",
  "auth.providers": [
    {
      "type": "saml",
      // Find this listed in your SAML Identity Provider's admin interface after you've added your SAML app.
      // It's sometimes called the "Identity Provider metadata URL" or "SAML metadata".
      "identityProviderMetadataURL": "https://example.com/saml-metadata"
    }
  ]
}
```

See the [`saml` auth provider documentation](../config/critical_config.md#saml) for the full set of configuration options.

> WARNING: When using SAML identity provider-initiated authentication, only 1 SAML auth provider is currently supported.

### SAML troubleshooting

Setting the env var `INSECURE_SAML_LOG_TRACES=1` on the `sourcegraph/server` Docker conatiner (or the `sourcegraph-frontend` pod if Sourcegraph is deployed to a Kubernetes cluster) causes all SAML requests and responses to be logged.

### Vendor-specific SAML instructions

- [Configuring SAML with OneLogin](saml_with_onelogin.md)
- [Configuring SAML with Microsoft Active Directory Federation Services (ADFS)](saml_with_microsoft_adfs.md)

### SAML Service Provider metadata

To make it easier to register Sourcegraph as a SAML Service Provider with your SAML Identity Provider, Sourcegraph exposes SAML Service Provider metadata at the URL path `/.auth/saml/metadata`. For example:

```
https://sourcegraph.example.com/.auth/saml/metadata
```

## HTTP authentication proxies

You can wrap Sourcegraph in an authentication proxy that authenticates the user and passes the user's username to Sourcegraph via HTTP headers. The most popular such authentication proxy is [bitly/oauth2_proxy](https://github.com/bitly/oauth2_proxy). Another example is [Google Identity-Aware Proxy (IAP)](https://cloud.google.com/iap/). Both work well with Sourcegraph.

To use an authentication proxy to authenticate users to Sourcegraph, add the following lines to your critical configuration:

```json
{
  // ...
  "auth.providers": [
    {
      "type": "http-header",
      "usernameHeader": "X-Forwarded-User"
    }
  ]
}
```

Replace `X-Forwarded-User` with the name of the HTTP header added by the authentication proxy that contains the user's username.

Ensure that the HTTP proxy is not setting its own `Authorization` header on the request. Sourcegraph rejects requests with unrecognized `Authorization` headers and prints the error log `lvl=eror msg="Invalid Authorization header." err="unrecognized HTTP Authorization request header scheme (supported values: token, token-sudo)"`.

For bitly/oauth2_proxy, use the `-pass-basic-auth false` option to prevent it from sending the `Authorization` header.

### Username header prefixes

Some proxies add a prefix to the username header value. For example, Google IAP sets the `x-goog-authenticated-user-id` to a value like `accounts.google.com:alice` rather than just `alice`. If this is the case, use the `stripUsernameHeaderPrefix` field. If using Google IAP, for example, add the following lines to your critical configuration:

```json
{
  // ...
  "auth.providers": [
    {
      "type": "http-header",
      "usernameHeader": "x-goog-authenticated-user-id",
      "stripUsernameHeaderPrefix": "accounts.google.com:"
    }
  ]
}
```

## Username normalization

Usernames on Sourcegraph are normalized according to the following rules.

- Any portion of the username after a '@' character is removed
- Any characters not in `[a-zA-Z0-9-]` are replaced with `-`
- Usernames with consecutive '-' characters are not allowed
- Usernames that start or end with '-' are not allowed

Usernames from authentication providers are normalized before being used in Sourcegraph. Usernames chosen by users are rejected if they do not meet these criteria.

For example, a user whose external username (according the authentication provider) is `alice.smith@example.com` would have the Sourcegraph username `alice-smith`.

If multiple accounts normalize into the same username, only the first user account is created. Other users won't be able to sign in. This is a rare occurrence; contact support if this is a blocker.
