# Web Application

## Local development

Use `sg` CLI tool to configure and start local development server. For more information check out [the `sg` documentation](https://docs.sourcegraph.com/dev/background-information/sg).

Our local development server runs by starting both a [Caddy](https://caddyserver.com/) HTTPS server and a Node HTTP server. We then can reverse proxy requests to the Node server to serve client assets under HTTPS.

### Configuration

Environment variables important for the web server:

1. `WEBPACK_SERVE_INDEX` should be set to `true` to enable `HTMLWebpackPlugin`.
2. `SOURCEGRAPH_API_URL` is used as a proxied API url. By default it points to the [https://k8s.sgdev.org](https://k8s.sgdev.org).

It's possible to overwrite these variables by creating `sg.config.overwrite.yaml` in the root folder and adjusting the `env` section of the relevant command.

### Development server

```sh
sg start web-standalone
```

For open-source version:

```sh
sg start oss-web-standalone
```

#### Public API

To use a public API that doesn't require authentication for most of the functionality:

```sh
SOURCEGRAPH_API_URL=https://sourcegraph.com sg start web-standalone
```

For open-source version:

```sh
SOURCEGRAPH_API_URL=https://sourcegraph.com sg start oss-web-standalone
```

### Production server

```sh
sg start web-standalone-prod
```

For open-source version:

```sh
sg start oss-web-standalone-prod
```

Web app should be available at `https://${SOURCEGRAPH_HTTPS_DOMAIN}:${SOURCEGRAPH_HTTPS_PORT}`. Build artifacts will be served from `<rootRepoPath>/ui/assets`.

Note: If you are unable to use the above commands (e.g. you can't install Caddy), you can use `sg run web-standalone-http` instead. This will start a development server using only Node, and will be available at `http://localhost:${SOURCEGRAPH_HTTP_PORT}`.

### API proxy

In both environments, server proxies API requests to `SOURCEGRAPH_API_URL` provided as the `.env` variable.

### esbuild (experimental)

See https://docs.sourcegraph.com/dev/background-information/web/build#esbuild.
