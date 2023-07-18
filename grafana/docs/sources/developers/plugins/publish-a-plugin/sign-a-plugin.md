---
aliases:
  - ../sign-a-plugin/
description: How to sign a Grafana plugin.
keywords:
  - grafana
  - plugins
  - plugin
  - sign plugin
  - signing plugin
labels:
  products:
    - enterprise
    - oss
title: Sign a plugin
weight: 400
---

# Sign a plugin

Grafana requires all plugins to be signed so that we can verify their authenticity with [signature verification]({{< relref "../../../administration/plugin-management#plugin-signatures" >}}).

All Grafana Labs-authored backend plugins, including Enterprise plugins, are signed. By [default]({{< relref "../../../administration/plugin-management#allow-unsigned-plugins" >}}), Grafana **requires** all plugins to be signed in order for them to be loaded.

Before you can sign your plugin, you need to decide whether you want to sign it as a _public_ or a _private_ plugin.

To make your plugin publicly available outside of your organization, sign your plugin under a _community_ or _commercial_ [signature level](#plugin-signature-levels). Public plugins are available from the [Grafana plugin catalog](/plugins) and can be installed by anyone.

If you intend to only use the plugin within your organization, sign it under a _private_ [signature level](#plugin-signature-levels).

## Generate an API key

To verify ownership of your plugin, generate an API key that you'll use every time you need to sign a new version of your plugin.

1. [Create a Grafana Cloud account](/signup).

1. Make sure that the first part of the plugin ID matches the slug of your Grafana Cloud account.

   You can find the plugin ID in the `plugin.json` file inside your plugin directory. For example, if your account slug is `acmecorp`, you need to prefix the plugin ID with `acmecorp-`.

1. [Create a Grafana Cloud API key](/docs/grafana-cloud/reference/create-api-key/) with the **PluginPublisher** role.

## Sign a public plugin

Public plugins need to be reviewed by the Grafana team before you can sign them.

1. Submit your plugin for [review]({{< relref "./publish-or-update-a-plugin.md#publish-your-plugin" >}}).
1. If we approve your plugin, you're granted a plugin signature level. You need this signature level to proceed.
1. In your plugin directory, sign the plugin with the API key you just created. Grafana Sign Plugin creates a [MANIFEST.txt](#plugin-manifest) file in the `dist` directory of your plugin:

   ```bash
   export GRAFANA_API_KEY=<YOUR_API_KEY>
   npx @grafana/sign-plugin@latest
   ```

## Sign a private plugin

1. In your plugin directory, sign the plugin with the API key you just created. Grafana Sign Plugin creates a [MANIFEST.txt](#plugin-manifest) file in the `dist` directory of your plugin.

   ```bash
   export GRAFANA_API_KEY=<YOUR_API_KEY>
   npx @grafana/sign-plugin@latest --rootUrls https://example.com/grafana
   ```

1. After the `rootUrls` flag, enter a comma-separated list of URLs for the Grafana instances where you intend to install the plugin.

## Plugin signature levels

To sign a plugin, you need to select the _signature level_ that you want to sign it under. The signature level of your plugin determines how you can distribute it.

You can sign your plugin under three different _signature levels_: _private_, _community_, and _commercial_.

| **Plugin Level** | **Paid Subscription Required?**                 | **Description**                                                                                                                                                                                                                                                                                                                  |
| ---------------- | ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Private          | No;<br>Free of charge                           | <p>You can create and sign a Private plugin for any technology at no charge.</p><p>Private plugins are intended for use on your own installation of Grafana. They may not be distributed to the Grafana community, and they are not published in the Grafana plugin catalog.</p>                                                 |
| Community        | No;<br>Free of charge                           | <p>You can create, sign, and distribute plugins at no charge, provided that all dependent technologies are open source and not for profit.</p><p>Community plugins are published in the official Grafana catalog, and are available to the entire Grafana community.</p>                                                         |
| Commercial       | Yes;<br>Commercial plugin subscription required | <p>You can create, sign, and distribute plugins with dependent technologies that are closed source or commercially backed. To do so, enter into a Commercial plugin subscription with Grafana Labs.</p><p>Commercial plugins are published on the Grafana plugin catalog, and are available to the entire Grafana community.</p> |

For instructions on how to sign a plugin under the Community and Commercial signature level, refer to [Sign a public plugin](#sign-a-public-plugin).

For instructions on how to sign a plugin under the Private signature level, refer to [Sign a private plugin](#sign-a-private-plugin).

## Plugin manifest

For Grafana to verify the digital signature of a plugin, the plugin must include a signed manifest file, `MANIFEST.txt`. The signed manifest file contains two sections:

- **Signed message -** Contains plugin metadata and plugin files with their respective checksums (SHA256).
- **Digital signature -** Created by encrypting the signed message using a private key. Grafana has a public key built-in that can be used to verify that the digital signature has been encrypted using the expected private key.

**Example**

```txt
-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

{
  "manifestVersion": "2.0.0",
  "signatureType": "community",
  "signedByOrg": "myorgid",
  "signedByOrgName": "My Org",
  "plugin": "myorgid-simple-panel",
  "version": "1.0.0",
  "time": 1602753404133,
  "keyId": "7e4d0c6a708866e7",
  "files": {
    "LICENSE": "12ab7a0961275f5ce7a428e662279cf49bab887d12b2ff7bfde738346178c28c",
    "module.js.LICENSE.txt": "0d8f66cd4afb566cb5b7e1540c68f43b939d3eba12ace290f18abc4f4cb53ed0",
    "module.js.map": "8a4ede5b5847dec1c6c30008d07bef8a049408d2b1e862841e30357f82e0fa19",
    "plugin.json": "13be5f2fd55bee787c5413b5ba6a1fae2dfe8d2df6c867dadc4657b98f821f90",
    "README.md": "2d90145b28f22348d4f50a81695e888c68ebd4f8baec731fdf2d79c8b187a27f",
    "module.js": "b4b6945bbf3332b08e5e1cb214a5b85c82557b292577eb58c8eb1703bc8e4577"
  }
}
-----BEGIN PGP SIGNATURE-----
Version: OpenPGP.js v4.10.1
Comment: https://openpgpjs.org

wqEEARMKAAYFAl+IE3wACgkQfk0ManCIZudpdwIHTCqjVzfm7DechTa7BTbd
+dNIQtwh8Tv2Q9HksgN6c6M9nbQTP0xNHwxSxHOI8EL3euz/OagzWoiIWulG
7AQo7FYCCQGucaLPPK3tsWaeFqVKy+JtQhrJJui23DAZLSYQYZlKQ+nFqc9x
T6scfmuhWC/TOcm83EVoCzIV3R5dOTKHqkjIUg==
=GdNq
-----END PGP SIGNATURE-----
```

## Troubleshooting

### Why do I get a "Modified signature" error?

In some cases an invalid `MANIFEST.txt` is generated because of an issue when signing the plugin on Windows. You can fix this by replacing all double backslashes, `\\`, with a forward slash, `/`, in the `MANIFEST.txt` file. You need to do this every time you sign your plugin.

### Why do I get a "Field is required: `rootUrls`" error for my public plugin?

With a **public** plugin, your plugin doesn't have a plugin signature level assigned to it yet. A Grafana team member will assign a signature level to your plugin once it has been reviewed and approved. For more information, refer to [Sign a public plugin](#sign-a-public-plugin).

### Why do I get a "Field is required: `rootUrls`" error for my private plugin?

With a **private** plugin, you need to add a `rootUrls` flag to the `plugin:sign` command. The `rootUrls` must match the [root_url]({{< relref "../../../setup-grafana/configure-grafana#root_url" >}}) configuration. For more information, refer to [Sign a private plugin](#sign-a-private-plugin).

If you still get this error, make sure that the API key was generated by a Grafana Cloud account that matches the first part of the plugin ID.
