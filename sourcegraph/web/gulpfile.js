// @ts-check

const log = require('fancy-log')
const gulp = require('gulp')
const createWebpackCompiler = require('webpack')
const WebpackDevServer = require('webpack-dev-server')
const { graphQLTypes, schema, watchGraphQLTypes, watchSchema } = require('../shared/gulpfile')
const webpackConfig = require('./webpack.config')

/**
 * Looks up an environment variable. Throws when not set and no default is
 * provided.
 */
function readEnvString(variable, defaultValue) {
  const value = process.env[variable]

  if (!value) {
    if (defaultValue === undefined) {
      throw new Error(`Environment variable ${variable} must be set.`)
    }
    return defaultValue
  }
  return value
}

const WEBPACK_STATS_OPTIONS = {
  all: false,
  timings: true,
  errors: true,
  warnings: true,
  colors: true,
}

/**
 * @param {import('webpack').Stats} stats
 */
const logWebpackStats = stats => {
  log(stats.toString(WEBPACK_STATS_OPTIONS))
}

async function webpack() {
  const compiler = createWebpackCompiler(webpackConfig)
  /** @type {import('webpack')} */
  const stats = await new Promise((resolve, reject) => {
    compiler.run((err, stats) => (err ? reject(err) : resolve(stats)))
  })
  logWebpackStats(stats)
  if (stats.hasErrors()) {
    throw Object.assign(new Error('Failed to compile'), { showStack: false })
  }
}

async function webpackDevServer() {
  const sockHost = readEnvString('SOURCEGRAPH_HTTPS_DOMAIN', 'sourcegraph.test')
  const sockPort = parseInt(readEnvString('SOURCEGRAPH_HTTPS_PORT', '3443'), 10)

  /** @type {import('webpack-dev-server').Configuration & { liveReload?: boolean }} */
  const options = {
    hot: !process.env.NO_HOT,
    inline: !process.env.NO_HOT,
    allowedHosts: ['.host.docker.internal'],
    host: 'localhost',
    port: 3080,
    publicPath: '/.assets/',
    contentBase: './ui/assets',
    stats: WEBPACK_STATS_OPTIONS,
    noInfo: false,
    disableHostCheck: true,
    proxy: {
      '/': {
        target: 'http://localhost:3081',
        // Avoid crashing on "read ECONNRESET".
        onError: err => console.error(err),
        onProxyReqWs: (_proxyReq, _req, socket) =>
          socket.on('error', err => console.error('WebSocket proxy error:', err)),
      },
    },
    sockHost,
    sockPort,
  }
  WebpackDevServer.addDevServerEntrypoints(webpackConfig, options)
  const server = new WebpackDevServer(createWebpackCompiler(webpackConfig), options)
  await new Promise((resolve, reject) => {
    server.listen(3080, '0.0.0.0', err => (err ? reject(err) : resolve()))
  })
}

/**
 * Builds everything.
 */
const build = gulp.parallel(gulp.series(gulp.parallel(schema, graphQLTypes), gulp.parallel(webpack)))

/**
 * Watches everything and rebuilds on file changes.
 */
const watch = gulp.series(
  // Ensure the typings that TypeScript depends on are build to avoid first-time-run errors
  gulp.parallel(schema, graphQLTypes),
  gulp.parallel(watchSchema, watchGraphQLTypes, webpackDevServer)
)

module.exports = { build, watch, webpackDevServer, webpack }
