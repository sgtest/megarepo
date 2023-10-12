---
aliases:
  - ../../../auth/generic-oauth/
description: Configure generic OAuth2 authentication
keywords:
  - grafana
  - configuration
  - documentation
  - oauth
labels:
  products:
    - cloud
    - enterprise
    - oss
menuTitle: Generic OAuth2
title: Configure generic OAuth2 authentication
weight: 700
---

# Configure generic OAuth2 authentication

{{< docs/shared lookup="auth/intro.md" source="grafana" version="<GRAFANA VERSION>" >}}

Grafana provides OAuth2 integrations for the following auth providers:

- [Azure AD OAuth]({{< relref "../azuread" >}})
- [GitHub OAuth]({{< relref "../github" >}})
- [GitLab OAuth]({{< relref "../gitlab" >}})
- [Google OAuth]({{< relref "../google" >}})
- [Grafana Com OAuth]({{< relref "../grafana-com" >}})
- [Keycloak OAuth]({{< relref "../keycloak" >}})
- [Okta OAuth]({{< relref "../okta" >}})

If your OAuth2 provider is not listed, you can use generic OAuth2 authentication.

This topic describes how to configure generic OAuth2 authentication and includes [examples of setting up generic OAuth2]({{< relref "#examples-of-setting-up-generic-oauth2" >}}) with specific OAuth2 providers.

## Before you begin

To follow this guide:

- Ensure that you have access to the [Grafana configuration file]({{< relref "../../../configure-grafana#configuration-file-location" >}}).
- Ensure you know how to create an OAuth2 application with your OAuth2 provider. Consult the documentation of your OAuth2 provider for more information.
- If you are using refresh tokens, ensure you know how to set them up with your OAuth2 provider. Consult the documentation of your OAuth2 provider for more information.

## Steps

To integrate your OAuth2 provider with Grafana using our generic OAuth2 authentication, follow these steps:

1. Create an OAuth2 application in your chosen OAuth2 provider.
1. Set the callback URL for your OAuth2 app to `http://<my_grafana_server_name_or_ip>:<grafana_server_port>/login/generic_oauth`.

   Ensure that the callback URL is the complete HTTP address that you use to access Grafana via your browser, but with the appended path of `/login/generic_oauth`.

   For the callback URL to be correct, it might be necessary to set the `root_url` option in the `[server]`section of the Grafana configuration file. For example, if you are serving Grafana behind a proxy.

1. Refer to the following table to update field values located in the `[auth.generic_oauth]` section of the Grafana configuration file:

   | Field                        | Description                                                                                                                                                                                       |
   | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
   | `client_id`, `client_secret` | These values must match the client ID and client secret from your OAuth2 app.                                                                                                                     |
   | `auth_url`                   | The authorization endpoint of your OAuth2 provider.                                                                                                                                               |
   | `api_url`                    | The user information endpoint of your OAuth2 provider. Information returned by this endpoint must be compatible with [OpenID UserInfo](https://connect2id.com/products/server/docs/api/userinfo). |
   | `enabled`                    | Enables generic OAuth2 authentication. Set this value to `true`.                                                                                                                                  |

   Review the list of other generic OAuth2 [configuration options]({{< relref "#configuration-options" >}}) and complete them, as necessary.

1. Optional: [Configure a refresh token]({{< relref "#configure-a-refresh-token" >}}):

   a. Enable `accessTokenExpirationCheck` feature toggle.

   b. Extend the `scopes` field of `[auth.generic_oauth]` section in Grafana configuration file with refresh token scope used by your OAuth2 provider.

   c. Set `use_refresh_token` to `true` in `[auth.generic_oauth]` section in Grafana configuration file.

   d. Enable the refresh token on the provider if required.

1. [Configure role mapping]({{< relref "#configure-role-mapping" >}}).
1. Optional: [Configure team synchronization]({{< relref "#configure-team-synchronization" >}}).
1. Restart Grafana.

   You should now see a generic OAuth2 login button on the login page and be able to log in or sign up with your OAuth2 provider.

## Configuration options

The following table outlines the various generic OAuth2 configuration options. You can apply these options as environment variables, similar to any other configuration within Grafana.

| Setting                      | Required | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Default         |
| ---------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| `enabled`                    | No       | Enables generic OAuth2 authentication.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | `false`         |
| `name`                       | No       | Name that refers to the generic OAuth2 authentication from the Grafana user interface.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | `OAuth`         |
| `icon`                       | No       | Icon used for the generic OAuth2 authentication in the Grafana user interface.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | `signin`        |
| `client_id`                  | Yes      | Client ID provided by your OAuth2 app.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |                 |
| `client_secret`              | Yes      | Client secret provided by your OAuth2 app.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |                 |
| `auth_url`                   | Yes      | Authorization endpoint of your OAuth2 provider.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |                 |
| `token_url`                  | Yes      | Endpoint used to obtain the OAuth2 access token.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |                 |
| `api_url`                    | Yes      | Endpoint used to obtain user information compatible with [OpenID UserInfo](https://connect2id.com/products/server/docs/api/userinfo).                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |                 |
| `auth_style`                 | No       | Name of the [OAuth2 AuthStyle](https://pkg.go.dev/golang.org/x/oauth2#AuthStyle) to be used when ID token is requested from OAuth2 provider. It determines how `client_id` and `client_secret` are sent to Oauth2 provider. Available values are `AutoDetect`, `InParams` and `InHeader`.                                                                                                                                                                                                                                                                                                                  | `AutoDetect`    |
| `scopes`                     | No       | List of comma- or space-separated OAuth2 scopes.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | `user:email`    |
| `empty_scopes`               | No       | Set to `true` to use an empty scope during authentication.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | `false`         |
| `allow_sign_up`              | No       | Controls Grafana user creation through the generic OAuth2 login. Only existing Grafana users can log in with generic OAuth if set to `false`.                                                                                                                                                                                                                                                                                                                                                                                                                                                              | `true`          |
| `auto_login`                 | No       | Set to `true` to enable users to bypass the login screen and automatically log in. This setting is ignored if you configure multiple auth providers to use auto-login.                                                                                                                                                                                                                                                                                                                                                                                                                                     | `false`         |
| `id_token_attribute_name`    | No       | The name of the key used to extract the ID token from the returned OAuth2 token.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | `id_token`      |
| `login_attribute_path`       | No       | [JMESPath](http://jmespath.org/examples.html) expression to use for user login lookup from the user ID token. For more information on how user login is retrieved, refer to [Configure login]({{< relref "#configure-login" >}}).                                                                                                                                                                                                                                                                                                                                                                          |                 |
| `name_attribute_path`        | No       | [JMESPath](http://jmespath.org/examples.html) expression to use for user name lookup from the user ID token. This name will be used as the user's display name. For more information on how user display name is retrieved, refer to [Configure display name]({{< relref "#configure-display-name" >}}).                                                                                                                                                                                                                                                                                                   |                 |
| `email_attribute_path`       | No       | [JMESPath](http://jmespath.org/examples.html) expression to use for user email lookup from the user information. For more information on how user email is retrieved, refer to [Configure email address]({{< relref "#configure-email-address" >}}).                                                                                                                                                                                                                                                                                                                                                       |                 |
| `email_attribute_name`       | No       | Name of the key to use for user email lookup within the `attributes` map of OAuth2 ID token. For more information on how user email is retrieved, refer to [Configure email address]({{< relref "#configure-email-address" >}}).                                                                                                                                                                                                                                                                                                                                                                           | `email:primary` |
| `role_attribute_path`        | No       | [JMESPath](http://jmespath.org/examples.html) expression to use for Grafana role lookup. Grafana will first evaluate the expression using the OAuth2 ID token. If no role is found, the expression will be evaluated using the user information obtained from the UserInfo endpoint. The result of the evaluation should be a valid Grafana role (`Viewer`, `Editor`, `Admin` or `GrafanaAdmin`). For more information on user role mapping, refer to [Configure role mapping]({{< relref "#configure-role-mapping" >}}).                                                                                  |                 |
| `role_attribute_strict`      | No       | Set to `true` to deny user login if the Grafana role cannot be extracted using `role_attribute_path`. For more information on user role mapping, refer to [Configure role mapping]({{< relref "#configure-role-mapping" >}}).                                                                                                                                                                                                                                                                                                                                                                              | `false`         |
| `allow_assign_grafana_admin` | No       | Set to `true` to enable automatic sync of the Grafana server administrator role. If this option is set to `true` and the result of evaluating `role_attribute_path` for a user is `GrafanaAdmin`, Grafana grants the user the server administrator privileges and organization administrator role. If this option is set to `false` and the result of evaluating `role_attribute_path` for a user is `GrafanaAdmin`, Grafana grants the user only organization administrator role. For more information on user role mapping, refer to [Configure role mapping]({{< relref "#configure-role-mapping" >}}). | `false`         |
| `skip_org_role_sync`         | No       | Set to `true` to stop automatically syncing user roles. This will allow you to set organization roles for your users from within Grafana manually.                                                                                                                                                                                                                                                                                                                                                                                                                                                         | `false`         |
| `groups_attribute_path`      | No       | [JMESPath](http://jmespath.org/examples.html) expression to use for user group lookup. Grafana will first evaluate the expression using the OAuth2 ID token. If no groups are found, the expression will be evaluated using the user information obtained from the UserInfo endpoint. The result of the evaluation should be a string array of groups.                                                                                                                                                                                                                                                     |                 |
| `allowed_groups`             | No       | List of comma- or space-separated groups. The user should be a member of at least one group to log in. If you configure `allowed_groups`, you must also configure `groups_attribute_path`.                                                                                                                                                                                                                                                                                                                                                                                                                 |                 |
| `allowed_organizations`      | No       | List of comma- or space-separated organizations. The user should be a member of at least one organization to log in.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |                 |
| `allowed_domains`            | No       | List comma- or space-separated domains. The user should belong to at least one domain to log in.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |                 |
| `team_ids`                   | No       | String list of team IDs. If set, the user must be a member of one of the given teams to log in. If you configure `team_ids`, you must also configure `teams_url` and `team_ids_attribute_path`.                                                                                                                                                                                                                                                                                                                                                                                                            |                 |
| `team_ids_attribute_path`    | No       | The [JMESPath](http://jmespath.org/examples.html) expression to use for Grafana team ID lookup within the results returned by the `teams_url` endpoint.                                                                                                                                                                                                                                                                                                                                                                                                                                                    |                 |
| `teams_url`                  | No       | The URL used to query for team IDs. If not set, the default value is `/teams`. If you configure `teams_url`, you must also configure `team_ids_attribute_path`.                                                                                                                                                                                                                                                                                                                                                                                                                                            |                 |
| `tls_skip_verify_insecure`   | No       | If set to `true`, the client accepts any certificate presented by the server and any host name in that certificate. _You should only use this for testing_, because this mode leaves SSL/TLS susceptible to man-in-the-middle attacks.                                                                                                                                                                                                                                                                                                                                                                     | `false`         |
| `tls_client_cert`            | No       | The path to the certificate.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |                 |
| `tls_client_key`             | No       | The path to the key.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |                 |
| `tls_client_ca`              | No       | The path to the trusted certificate authority list.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |                 |
| `use_pkce`                   | No       | Set to `true` to use [Proof Key for Code Exchange (PKCE)](https://datatracker.ietf.org/doc/html/rfc7636). Grafana uses the SHA256 based `S256` challenge method and a 128 bytes (base64url encoded) code verifier.                                                                                                                                                                                                                                                                                                                                                                                         | `false`         |
| `use_refresh_token`          | No       | Set to `true` to use refresh token and check access token expiration. The `accessTokenExpirationCheck` feature toggle should also be enabled to use refresh token.                                                                                                                                                                                                                                                                                                                                                                                                                                         | `false`         |

### Configure login

Grafana can resolve a user's login from the OAuth2 ID token or user information retrieved from the OAuth2 UserInfo endpoint.
Grafana looks at these sources in the order listed until it finds a login.
If no login is found, then the user's login is set to user's email address.

Refer to the following table for information on what to configure based on how your Oauth2 provider returns a user's login:

| Source of login                                                                 | Required configuration                           |
| ------------------------------------------------------------------------------- | ------------------------------------------------ |
| `login` or `username` field of the OAuth2 ID token.                             | N/A                                              |
| Another field of the OAuth2 ID token.                                           | Set `login_attribute_path` configuration option. |
| `login` or `username` field of the user information from the UserInfo endpoint. | N/A                                              |
| Another field of the user information from the UserInfo endpoint.               | Set `login_attribute_path` configuration option. |

### Configure display name

Grafana can resolve a user's display name from the OAuth2 ID token or user information retrieved from the OAuth2 UserInfo endpoint.
Grafana looks at these sources in the order listed until it finds a display name.
If no display name is found, then user's login is displayed instead.

Refer to the following table for information on what you need to configure depending on how your Oauth2 provider returns a user's name:

| Source of display name                                                             | Required configuration                          |
| ---------------------------------------------------------------------------------- | ----------------------------------------------- |
| `name` or `display_name` field of the OAuth2 ID token.                             | N/A                                             |
| Another field of the OAuth2 ID token.                                              | Set `name_attribute_path` configuration option. |
| `name` or `display_name` field of the user information from the UserInfo endpoint. | N/A                                             |
| Another field of the user information from the UserInfo endpoint.                  | Set `name_attribute_path` configuration option. |

### Configure email address

Grafana can resolve the user's email address from the OAuth2 ID token, the user information retrieved from the OAuth2 UserInfo endpoint, or the OAuth2 `/emails` endpoint.
Grafana looks at these sources in the order listed until an email address is found.
If no email is found, then the email address of the user is set to an empty string.

Refer to the following table for information on what to configure based on how the Oauth2 provider returns a user's email address:

| Source of email address                                                                                                                                                 | Required configuration                                                                                             |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `email` field of the OAuth2 ID token.                                                                                                                                   | N/A                                                                                                                |
| `attributes` map of the OAuth2 ID token.                                                                                                                                | Set `email_attribute_name` configuration option. By default, Grafana searches for email under `email:primary` key. |
| `upn` field of the OAuth2 ID token.                                                                                                                                     | N/A                                                                                                                |
| `email` field of the user information from the UserInfo endpoint.                                                                                                       | N/A                                                                                                                |
| Another field of the user information from the UserInfo endpoint.                                                                                                       | Set `email_attribute_path` configuration option.                                                                   |
| Email address marked as primary from the `/emails` endpoint of <br /> the OAuth2 provider (obtained by appending `/emails` to the URL <br /> configured with `api_url`) | N/A                                                                                                                |

## Configure a refresh token

> **Note:** This feature is behind the `accessTokenExpirationCheck` feature toggle.

When a user logs in using an OAuth2 provider, Grafana verifies that the access token has not expired. When an access token expires, Grafana uses the provided refresh token (if any exists) to obtain a new access token.

Grafana uses a refresh token to obtain a new access token without requiring the user to log in again. If a refresh token doesn't exist, Grafana logs the user out of the system after the access token has expired.

To configure generic OAuth2 to use a refresh token, set `use_refresh_token` configuration option to `true` and perform one or both of the following steps, if required:

1. Extend the `scopes` field of `[auth.generic_oauth]` section in Grafana configuration file with additional scopes.
1. Enable the refresh token on the provider.

> **Note:** The `accessTokenExpirationCheck` feature toggle will be removed in Grafana v10.2.0 and the `use_refresh_token` configuration value will be used instead for configuring refresh token fetching and access token expiration check.

## Configure role mapping

Unless `skip_org_role_sync` option is enabled, the user's role will be set to the role retrieved from the auth provider upon user login.

The user's role is retrieved using a [JMESPath](http://jmespath.org/examples.html) expression from the `role_attribute_path` configuration option.
To map the server administrator role, use the `allow_assign_grafana_admin` configuration option.
Refer to [configuration options]({{< relref "#configuration-options" >}}) for more information.

If no valid role is found, the user is assigned the role specified by [the `auto_assign_org_role` option]({{< relref "../../../configure-grafana#auto_assign_org_role" >}}).
You can disable this default role assignment by setting `role_attribute_strict = true`.
This setting denies user access if no role or an invalid role is returned.

To ease configuration of a proper JMESPath expression, go to [JMESPath](http://jmespath.org/) to test and evaluate expressions with custom payloads.

### Role mapping examples

This section includes examples of JMESPath expressions used for role mapping.

#### Map user organization role

In this example, the user has been granted the role of an `Editor`. The role assigned is based on the value of the property `role`, which must be a valid Grafana role such as `Admin`, `Editor`, `Viewer` or `None`.

Payload:

```json
{
    ...
    "role": "Editor",
    ...
}
```

Config:

```bash
role_attribute_path = role
```

In the following more complex example, the user has been granted the `Admin` role. This is because they are a member of the `admin` group of their OAuth2 provider.
If the user was a member of the `editor` group, they would be granted the `Editor` role, otherwise `Viewer`.

Payload:

```json
{
    ...
    "info": {
        ...
        "groups": [
            "engineer",
            "admin",
        ],
        ...
    },
    ...
}
```

Config:

```bash
role_attribute_path = contains(info.groups[*], 'admin') && 'Admin' || contains(info.groups[*], 'editor') && 'Editor' || 'Viewer'
```

#### Map server administrator role

In the following example, the user is granted the Grafana server administrator role.

Payload:

```json
{
    ...
    "info": {
        ...
        "roles": [
            "admin",
        ],
        ...
    },
    ...
}
```

Config:

```ini
role_attribute_path = contains(info.roles[*], 'admin') && 'GrafanaAdmin' || contains(info.roles[*], 'editor') && 'Editor' || 'Viewer'
allow_assign_grafana_admin = true
```

## Configure team synchronization

> **Note:** Available in [Grafana Enterprise]({{< relref "../../../../introduction/grafana-enterprise" >}}) and [Grafana Cloud](/docs/grafana-cloud/).

By using Team Sync, you can link your OAuth2 groups to teams within Grafana. This will automatically assign users to the appropriate teams.
Teams for each user are synchronized when the user logs in.

Generic OAuth2 groups can be referenced by group ID, such as `8bab1c86-8fba-33e5-2089-1d1c80ec267d` or `myteam`.
For information on configuring OAuth2 groups with Grafana using the `groups_attribute_path` configuration option, refer to [configuration options]({{< relref "#configuration-options" >}}).

To learn more about Team Sync, refer to [Configure team sync]({{< relref "../../configure-team-sync" >}}).

### Team synchronization example

Configuration:

```bash
groups_attribute_path = info.groups
```

Payload:

```json
{
    ...
    "info": {
        ...
        "groups": [
            "engineers",
            "analysts",
        ],
        ...
    },
    ...
}
```

## Examples of setting up generic OAuth2

This section includes examples of setting up generic OAuth2 integration.

### Set up OAuth2 with Auth0

To set up generic OAuth2 authentication with Auth0, follow these steps:

1. Create an Auth0 application using the following parameters:

   - Name: Grafana
   - Type: Regular Web Application

1. Go to the **Settings** tab of the application and set **Allowed Callback URLs** to `https://<grafana domain>/login/generic_oauth`.

1. Click **Save Changes**.

1. Update the `[auth.generic_oauth]` section of the Grafana configuration file using the values from the **Settings** tab:

   ```bash
   [auth.generic_oauth]
   enabled = true
   allow_sign_up = true
   auto_login = false
   team_ids =
   allowed_organizations =
   name = Auth0
   client_id = <client id>
   client_secret = <client secret>
   scopes = openid profile email offline_access
   auth_url = https://<domain>/authorize
   token_url = https://<domain>/oauth/token
   api_url = https://<domain>/userinfo
   use_pkce = true
   use_refresh_token = true
   ```

### Set up OAuth2 with Bitbucket

To set up generic OAuth2 authentication with Bitbucket, follow these steps:

1. Navigate to **Settings > Workspace setting > OAuth consumers** in BitBucket.

1. Create an application by selecting **Add consumer** and using the following parameters:

   - Allowed Callback URLs: `https://<grafana domain>/login/generic_oauth`

1. Click **Save**.

1. Update the `[auth.generic_oauth]` section of the Grafana configuration file using the values from the `Key` and `Secret` from the consumer description:

   ```bash
   [auth.generic_oauth]
   name = BitBucket
   enabled = true
   allow_sign_up = true
   auto_login = false
   client_id = <client key>
   client_secret = <client secret>
   scopes = account email
   auth_url = https://bitbucket.org/site/oauth2/authorize
   token_url = https://bitbucket.org/site/oauth2/access_token
   api_url = https://api.bitbucket.org/2.0/user
   teams_url = https://api.bitbucket.org/2.0/user/permissions/workspaces
   team_ids_attribute_path = values[*].workspace.slug
   team_ids =
   allowed_organizations =
   use_refresh_token = true
   ```

By default, a refresh token is included in the response for the **Authorization Code Grant**.

### Set up OAuth2 with OneLogin

To set up generic OAuth2 authentication with OneLogin, follow these steps:

1. Create a new Custom Connector in OneLogin with the following settings:

   - Name: Grafana
   - Sign On Method: OpenID Connect
   - Redirect URI: `https://<grafana domain>/login/generic_oauth`
   - Signing Algorithm: RS256
   - Login URL: `https://<grafana domain>/login/generic_oauth`

1. Add an app to the Grafana Connector:

   - Display Name: Grafana

1. Update the `[auth.generic_oauth]` section of the Grafana configuration file using the client ID and client secret from the **SSO** tab of the app details page:

   Your OneLogin Domain will match the URL you use to access OneLogin.

   ```bash
   [auth.generic_oauth]
   name = OneLogin
   enabled = true
   allow_sign_up = true
   auto_login = false
   client_id = <client id>
   client_secret = <client secret>
   scopes = openid email name
   auth_url = https://<onelogin domain>.onelogin.com/oidc/2/auth
   token_url = https://<onelogin domain>.onelogin.com/oidc/2/token
   api_url = https://<onelogin domain>.onelogin.com/oidc/2/me
   team_ids =
   allowed_organizations =
   ```
