# Site configuration

Site configuration defines how various Sourcegraph features behave. See the [full reference](#reference) below for a comprehensive list of site configuration options.

## View and edit site configuration

Site admins can view and edit site configuration on a Sourcegraph instance:

1. Go to **User menu > Site admin**.
1. Open the **Configuration** page. (The URL is `https://sourcegraph.example.com/site-admin/configuration`.)

## Reference

All site configuration options and their default values are shown below.

<div markdown-func=jsonschemadoc jsonschemadoc:path="admin/config/site.schema.json">[View page on docs.sourcegraph.com](https://docs.sourcegraph.com/admin/config/site_config) to see rendered content.</div>

#### Known bugs

The following site configuration options require the server to be restarted for the changes to take effect:

```
auth.accessTokens
auth.sessionExpiry
git.cloneURLToRepositoryName
searchScopes
extensions
disablePublicRepoRedirects
```

## Editing your site configuration if you cannot access the web UI

If you are having trouble accessing the web UI, you can make edits to your site configuration by editing a file in the Docker container using the following commands:

#### Single-container Docker instances

For Sourcegraph v3.12.1+ use:

```sh
docker exec -it $CONTAINER sh -c 'apk add --no-cache nano && nano ~/site-config.json'
```

Or if you prefer using a Vim editor:

```sh
docker exec -it $CONTAINER sh -c 'vi ~/site-config.json'
```

> **Note:** Not running Sourcegraph v3.12.1+? Use the following:
```sh
docker exec -it $CONTAINER sh -c 'apk add --no-cache nano && nano /site-config.json'
```
> Or if you prefer using a Vim editor:
```sh
docker exec -it $CONTAINER sh -c 'vi /site-config.json'
```

#### Kubernetes cluster instances

```sh
kubectl exec -it $FRONTEND_POD -- sh -c 'apk add --no-cache nano && nano ~/site-config.json'
```

Or if you prefer using a Vim editor:

```sh
kubectl exec -it $FRONTEND_POD -- sh -c 'vi ~/site-config.json'
```

> **Note:** Not running Sourcegraph v3.12.1+? Use the following:
```
kubectl exec -it $FRONTEND_POD -- sh -c 'apk add --no-cache nano && nano /site-config.json'
```

> Or if you prefer using a Vim editor:
```sh
kubectl exec -it $FRONTEND_POD -- sh -c 'vi /site-config.json'
```

Then simply save your changes (type <kbd>ctrl+x</kbd> and <kbd>y</kbd> to exit `nano` and save your changes). Your changes will be applied immediately in the same was as if you had made them through the web UI.

#### If you are still encountering issues

You can check the container logs to see if you have made any typos or mistakes in editing the configuration file. If you are still encountering problems, you can save the default site configuration that comes with Sourcegraph (below) or contact support@sourcegraph.com with any questions you have.

```
{
	// The externally accessible URL for Sourcegraph (i.e., what you type into your browser)
	// This is required to be configured for Sourcegraph to work correctly.
	// "externalURL": "https://sourcegraph.example.com",

	// The authentication provider to use for identifying and signing in users.
	// Only one entry is supported.
	//
	// The builtin auth provider with signup disallowed (shown below) means that
	// after the initial site admin signs in, all other users must be invited.
	//
	// Other providers are documented at https://docs.sourcegraph.com/admin/auth.
	"auth.providers": [
		{
			"type": "builtin",
			"allowSignup": false
		}
	],

	"search.index.enabled": true
}
```
