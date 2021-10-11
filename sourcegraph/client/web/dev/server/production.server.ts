import chalk from 'chalk'
import compression from 'compression'
import historyApiFallback from 'connect-history-api-fallback'
import express, { RequestHandler } from 'express'
import { createProxyMiddleware } from 'http-proxy-middleware'
import signale from 'signale'

import {
    PROXY_ROUTES,
    getAPIProxySettings,
    getCSRFTokenCookieMiddleware,
    environmentConfig,
    getCSRFTokenAndCookie,
    STATIC_ASSETS_PATH,
    STATIC_INDEX_PATH,
    WEB_SERVER_URL,
    shouldCompressResponse,
} from '../utils'

const { SOURCEGRAPH_API_URL, SOURCEGRAPH_HTTPS_PORT } = environmentConfig

async function startProductionServer(): Promise<void> {
    if (!SOURCEGRAPH_API_URL) {
        throw new Error('production.server.ts only supports *web-standalone* usage')
    }

    // Get CSRF token value from the `SOURCEGRAPH_API_URL`.
    const { csrfContextValue, csrfCookieValue } = await getCSRFTokenAndCookie(SOURCEGRAPH_API_URL)
    signale.await('Production server', { ...environmentConfig, csrfContextValue, csrfCookieValue })

    const app = express()

    // Compress all HTTP responses
    app.use(compression({ filter: shouldCompressResponse }))
    // Serve index.html in place of any 404 responses.
    app.use(historyApiFallback() as RequestHandler)
    // Attach `CSRF_COOKIE_NAME` cookie to every response to avoid "CSRF token is invalid" API error.
    app.use(getCSRFTokenCookieMiddleware(csrfCookieValue))

    // Serve build artifacts.
    app.use('/.assets', express.static(STATIC_ASSETS_PATH))

    // Proxy API requests to the `process.env.SOURCEGRAPH_API_URL`.
    app.use(
        PROXY_ROUTES,
        createProxyMiddleware(
            getAPIProxySettings({
                // Attach `x-csrf-token` header to every proxy request.
                csrfContextValue,
                apiURL: SOURCEGRAPH_API_URL,
            })
        )
    )

    // Redirect remaining routes to index.html
    app.get('/*', (_request, response) => response.sendFile(STATIC_INDEX_PATH))

    app.listen(SOURCEGRAPH_HTTPS_PORT, () => {
        signale.success(`Production server is ready at ${chalk.blue.bold(WEB_SERVER_URL)}`)
    })
}

startProductionServer().catch(error => signale.error(error))
