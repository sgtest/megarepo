# Loading configuration via the file system (advanced)

Some teams require Sourcegraph configuration to be stored in version control as opposed to editing via the Site admin UI.

As of Sourcegraph v3.4+, this is possible for [site configuration](site_config.md), [code host configuration](../external_service/index.md), and global settings.

## Benefits

1. Configuration can be checked into version control (e.g., Git).
1. Configuration is enforced across the entire instance, and edits cannot be made via the web UI (by default).

## Drawbacks

Loading configuration in this manner has two significant drawbacks:

1. You will no longer be able to save configuration edits through the web UI by default (you can use the web UI as scratch space, though).
1. Sourcegraph sometimes performs automatic migrations of configuration when upgrading versions. This process will now be more manual for you (see below).

## Site configuration

Set the environment variable below on all `frontend` containers (cluster deployment) or on the `server` container (single-container Docker deployment):

```bash
SITE_CONFIG_FILE=site.json
```

`site.json` contains the [site configuration](site_config.md), which you would otherwise edit through the in-app site configuration editor.

If you want to _allow_ edits to be made through the web UI (which will be overwritten with what is in the file on a subsequent restart), you may additionally set `SITE_CONFIG_ALLOW_EDITS=true`. **Note** that if you do enable this, it is your responsibility to ensure the configuration on your instance and in the file remain in sync.

## Code host configuration

Set the environment variable below on all `frontend` containers (cluster deployment) or on the `server` container (single-container Docker deployment):

```bash
EXTSVC_CONFIG_FILE=extsvc.json
```

`extsvc.json` contains a JSON object that specifies _all_ of your code hosts in a single JSONC file:

```jsonc

{
  "GITHUB": [
    {
      // First GitHub code host configuration: literally the JSON object from the code host config editor.
      "authorization": {},
      "url": "https://github.com",
      "token": "...",
      "repositoryQuery": ["affiliated"]
    },
    {
      // Another GitHub code host configuration.
      ...
    },
  ],
  "OTHER": [
    {
      // First "Generic Git host" code host configuration.
      "url": "https://mycodehost.example.com/repos",
      "repos": ["foo"],
    }
  ],
  "PHABRICATOR": [
    {
      // Phabricator code host configuration.
      ...
    },
  ]
}
```

You can find a full list of [valid top-level keys here](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@b7ebb9024e3a95109fdedfb8057795b9a7c638bc/-/blob/cmd/frontend/graphqlbackend/schema.graphql#L1104-1110).

If you want to _allow_ edits to be made through the web UI (which will be overwritten with what is in the file on a subsequent restart), you may additionally set `EXTSVC_CONFIG_ALLOW_EDITS=true`. **Note** that if you do enable this, it is your responsibility to ensure the configuration on your instance and in the file remain in sync.

## Global settings

Set the environment variable below on all `frontend` containers (cluster deployment) or on the `server` container (single-container Docker deployment):

```bash
GLOBAL_SETTINGS_FILE=global-settings.json
```

`global-settings.json` contains the global settings, which you would otherwise edit through the in-app global settings editor.

If you want to _allow_ edits to be made through the web UI (which will be overwritten with what is in the file on a subsequent restart), you may additionally set `GLOBAL_SETTINGS_ALLOW_EDITS=true`. Note that if you do enable this, it is your responsibility to ensure the global settings on your instance and in the file remain in sync.

## Upgrades and Migrations

As mentioned earlier, when configuration is loaded via the filesystem, Sourcegraph can no longer persist the automatic migrations to configuration it may perform when upgrading.

It will still perform such migrations on the configuration loaded from file, it just cannot persist such migrations **back to file**.

When you upgrade Sourcegraph, you should do the following to ensure your configurations do not become invalid:

1. Upgrade Sourcegraph to the new version
1. Visit each configuration page in the web UI (management console, site configuration, each code host)
1. Copy the (now migrated) configuration from those pages into your JSON files.

It is essential to follow the above steps after **every** Sourcegraph version update, because we only guarantee migrations remain valid across two minor versions. If you fail to apply a migration and later upgrade Sourcegraph twice more, you may effectively "skip" an important migration.

We're planning to improve this by having Sourcegraph notify you as a site admin when you should do the above, since today it is not actually required in most upgrades. See https://github.com/sourcegraph/sourcegraph/issues/4650 for details. In the meantime, we will do our best to communicate when this is needed to you through the changelog.

## Kubernetes ConfigMap

Currently, site admins are responsible for creating the ConfigMap resource that maps the above environment variables to files on the container disk. If you require assistance, please [contact us](mailto:support@sourcegraph.com).
