import shelljs from 'shelljs'
import signale from 'signale'
import webpack from 'webpack'

import { config } from '../config/webpack/production.config'

import { buildNpm } from './build-npm'
import * as tasks from './tasks'

const buildChrome = tasks.buildChrome('prod')
const buildFirefox = tasks.buildFirefox('prod')
const buildSafari = tasks.buildSafari('prod')
const buildEdge = tasks.buildEdge('prod')

tasks.copyAssets()

const compiler = webpack(config)

signale.await('Webpack compilation')

compiler.run(async (error, stats) => {
    console.log(stats?.toString(tasks.WEBPACK_STATS_OPTIONS))

    if (stats?.hasErrors()) {
        signale.error('Webpack compilation error')
        process.exit(1)
    }
    signale.success('Webpack compilation done')

    buildChrome()
    buildEdge()
    buildFirefox()
    if (isXcodeAvailable()) {
        buildSafari()
    } else {
        signale.debug('Skipping Safari build because Xcode tools were not found (xcrun, xcodebuild)')
    }
    tasks.copyIntegrationAssets()
    await buildNpm()
    signale.success('Build done')
})

function isXcodeAvailable(): boolean {
    return !!shelljs.which('xcrun') && !!shelljs.which('xcodebuild')
}
