import fs from 'fs'
import path from 'path'

import * as esbuild from 'esbuild'

import { WebpackManifest, WEBPACK_MANIFEST_PATH } from '../utils'

export const assetPathPrefix = '/.assets'

export const getManifest = (): WebpackManifest => ({
    'app.js': path.join(assetPathPrefix, 'scripts/app.js'),
    'app.css': path.join(assetPathPrefix, 'scripts/app.css'),
    isModule: true,
})

const writeManifest = async (manifest: WebpackManifest): Promise<void> => {
    await fs.promises.writeFile(WEBPACK_MANIFEST_PATH, JSON.stringify(manifest, null, 2))
}

/**
 * An esbuild plugin to write a webpack.manifest.json file (just as Webpack does), for compatibility
 * with our current Webpack build.
 */
export const manifestPlugin: esbuild.Plugin = {
    name: 'manifest',
    setup: build => {
        build.onStart(async () => {
            // The bug https://github.com/evanw/esbuild/issues/1384 means that onEnd isn't called in
            // serve mode, so write it here instead of waiting for onEnd. This is OK because we
            // don't actually need any information that's only available in onEnd.
            await writeManifest(getManifest())
        })
    },
}
