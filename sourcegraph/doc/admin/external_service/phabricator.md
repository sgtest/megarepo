# Phabricator

Site admins can link and sync Git repositories on [Phabricator](https://phabricator.org) with Sourcegraph so that users can search and navigate the repositories.

To set this up, add Phabricator as an external service to Sourcegraph:

1. Go to **User menu > Site admin**.
1. Open the **External services** page.
1. Press **+ Add external service**.
1. Enter a **Display name** (using "Phabricator" is OK if you only have one Phabricator instance).
1. In the **Kind** menu, select **Phabricator**.
1. Configure the connection to Phabricator in the JSON editor. Use Cmd/Ctrl+Space for completion, and [see configuration documentation below](#configuration).
1. Press **Add external service**.

## Repository linking and syncing

If you mirror your source repositories on Phabricator, Sourcegraph can provide users with links to various Phabricator pages if you add Phabricator as an external service (in **Site admin > External services**).

A Phabricator external service configuration consists of the following fields:

- `url` field that maps to the url of the Phabricator host
- `token` an optional Conduit API token, which you may generate from the Phabricator web interface. The token is used to fetch the list of repos available on the Phabricator installation
- `repos` if your Phabricator installation mirrors repositories from a different origin than Sourcegraph, you must specify a list of repository `path`s (as displayed on Sourcegraph) and their corresponding Phabricator `callsign`s. For example: `[{ path: 'gitolite.example.org/foobar', callsign: 'FOO'}]`. _Note that the `callsign` is case sensitive._

At least one of token and repos should be provided.

For example:

```json
{
  // ...
  "phabricator": [
    {
      "url": "https://phabricator.example.com",
      "token": "api-abcdefghijklmnop",
      "repos": [{ "path": "gitolite.example.com/mux", "callsign": "MUX" }]
    }
  ]
  // ...
}
```

See [configuration documentation](#configuration) below for more information.

### Troubleshooting

If your outbound links to Phabricator are not present or not working, verify your Sourcegraph repository path matches the "normalized" URI output by Phabricator's `diffusion.repository.search` conduit API.

For example, if you have a repository on Sourcegraph whose URL is `https://sourcegraph.example.com/path/to/repo` then you should see a URI returned from `diffusion.repository.search` whose `normalized` field is `path/to/repo`. Check this by navigating to `$PHABRICATOR_URL/conduit/method/diffusion.repository.search/` and use the "Call Method" form with `attachments` field set to `{ "uris": true }` and `constraints` field set to `{ "callsigns": ["$CALLSIGN_FOR_REPO_ON_SOURCEGRAPH"]}`. In the generated output, verify that the first URI has a normalized path equal to `path/to/repo`.

## Native extension

For production usage, we recommend installing the Sourcegraph Phabricator extension for all users (so that each user doesn't need to install the browser extension individually). This involves adding a new extension to the extension directory of your Phabricator instance.

See the [phabricator-extension](https://github.com/sourcegraph/phabricator-extension) repository for installation instructions and configuration settings.

## Configuration

<div markdown-func=jsonschemadoc jsonschemadoc:path="admin/external_service/phabricator.schema.json">[View page on docs.sourcegraph.com](https://docs.sourcegraph.com/admin/external_service/phabricator) to see rendered content.</div>
