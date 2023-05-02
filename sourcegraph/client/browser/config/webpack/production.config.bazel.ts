import TerserPlugin from 'terser-webpack-plugin'
import * as webpack from 'webpack'

import { config as baseConfig } from './base.config.bazel'
import { generateBundleUID } from './utils'

const { plugins, ...base } = baseConfig

export const config: webpack.Configuration = {
    ...base,
    mode: 'production',
    optimization: {
        minimize: true,
        minimizer: [
            new TerserPlugin({
                extractComments: false,
                terserOptions: {
                    output: {
                        // Without this, Uglify will change \u0000 to \0 (NULL byte),
                        // which causes Chrome to complain that the bundle is not UTF8
                        ascii_only: true,
                        beautify: false,
                        comments: false,
                    },
                },
            }) as webpack.WebpackPluginInstance,
        ],
    },
    plugins: (plugins || []).concat(
        ...[
            new webpack.DefinePlugin({
                'process.env': {
                    NODE_ENV: JSON.stringify('production'),
                    BUNDLE_UID: JSON.stringify(generateBundleUID()),
                    USE_EXTENSIONS: JSON.stringify(process.env.USE_EXTENSIONS),
                },
            }),
            new webpack.ProvidePlugin({
                $: 'jquery',
                jQuery: 'jquery',
                '$.fn.pjax': 'jquery-pjax',
            }),
        ]
    ),
}

// eslint-disable-next-line import/no-default-export
export default config
