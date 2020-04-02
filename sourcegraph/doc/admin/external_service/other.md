# Other Git repository hosts

Site admins can sync Git repositories on any Git repository host (by Git clone URL) with Sourcegraph so that users can search and navigate the repositories. Use this method only when your repository host is not named as a supported [code host](index.md).

To connect generic Git host to Sourcegraph:

1. Go to **Site admin > Manage repositories > Add repositories**
1. Select **Generic Git host**.
1. Configure the connection to generic Git host the action buttons above the text field, and additional fields can be added using <kbd>Cmd/Ctrl+Space</kbd> for auto-completion. See the [configuration documentation below](#configuration).
1. Press **Add repositories**.

## Constructing the `url` for SSH access

If your code host serves git repositories over SSH (e.g. Gerrit), make sure your Sourcegraph instance can connect to your code host over SSH:

```
docker exec $CONTAINER ssh -p $PORT $USER@$HOSTNAME
```

- $CONTAINER is the name or ID of your sourcegraph/server container
- $PORT is the port on which your code host's git server is listening for connections (Gerrit defaults to `29418`)
- $USER is your user on your code host (Gerrit defaults to `admin`)
- $HOSTNAME is the hostname of your code host from within the sourcegraph/server container (e.g. `gerrit.example.com`)

Here's an example for Gerrit:

```
docker exec sourcegraph ssh -p 29418 admin@gerrit.example.com
```

The `url` field is then

```json
  "url": "ssh://$USER@$HOSTNAME:$PORT"`
```

Here's an example for Gerrit:

```json
  "url": "ssh://admin@gerrit.example.com:29418",
```

## Adding repositories

For Gerrit, elements of the `repos` field are the same as the repository names. For example, a repository at https://gerrit.example.com/admin/repos/gorilla/mux will be `"gorilla/mux"` in the `repos` field.

Repositories must be listed individually:

```json
  "repos": [
    "gorilla/mux",
    "sourcegraph/sourcegraph"
  ]
```

## Configuration

<div markdown-func=jsonschemadoc jsonschemadoc:path="admin/external_service/other_external_service.schema.json">[View page on docs.sourcegraph.com](https://docs.sourcegraph.com/admin/external_service/other) to see rendered content.</div>
