import * as path from 'path'
import * as webpack from 'webpack'
import baseConfig from './base.config'
import { generateBundleUID } from './utils'

const { plugins, entry, ...base } = baseConfig

const entries = entry as webpack.Entry

const entriesWithAutoReload = {
    ...entries,
    background: [path.join(__dirname, '../../src/extension/scripts/auto-reloading.ts'), ...entries.background],
}

export default {
    ...base,
    entry: process.env.AUTO_RELOAD === 'false' ? entries : entriesWithAutoReload,
    mode: 'development',
    devtool: 'cheap-module-source-map',
    plugins: (plugins || []).concat(
        ...[
            new webpack.DefinePlugin({
                'process.env': {
                    NODE_ENV: JSON.stringify('development'),
                    BUNDLE_UID: JSON.stringify(generateBundleUID()),
                },
            }),
        ]
    ),
} as webpack.Configuration
