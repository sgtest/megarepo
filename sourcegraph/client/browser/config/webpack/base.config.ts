import MiniCssExtractPlugin from 'mini-css-extract-plugin'
import OptimizeCssAssetsPlugin from 'optimize-css-assets-webpack-plugin'
import * as path from 'path'
import * as webpack from 'webpack'

import { commonStylesheetLoaders, jsRule, tsRule } from '../shared/webpack'

const buildEntry = (...files: string[]) => files.map(file => path.join(__dirname, file))

const contentEntry = '../../src/config/content.entry.js'
const backgroundEntry = '../../src/config/background.entry.js'
const optionsEntry = '../../src/config/options.entry.js'
const pageEntry = '../../src/config/page.entry.js'
const extEntry = '../../src/config/extension.entry.js'

const config: webpack.Configuration = {
    entry: {
        background: buildEntry(extEntry, backgroundEntry, '../../src/extension/scripts/background.tsx'),
        options: buildEntry(extEntry, optionsEntry, '../../src/extension/scripts/options.tsx'),
        inject: buildEntry(extEntry, contentEntry, '../../src/extension/scripts/inject.tsx'),
        phabricator: buildEntry(pageEntry, '../../src/libs/phabricator/extension.tsx'),

        bootstrap: path.join(__dirname, '../../../../node_modules/bootstrap/dist/css/bootstrap.css'),
        style: path.join(__dirname, '../../src/app.scss'),
    },
    output: {
        path: path.join(__dirname, '../../build/dist/js'),
        filename: '[name].bundle.js',
        chunkFilename: '[id].chunk.js',
    },

    plugins: [
        new MiniCssExtractPlugin({ filename: '../css/[name].bundle.css' }) as any, // @types package is incorrect
        new OptimizeCssAssetsPlugin(),
    ],
    resolve: {
        extensions: ['.ts', '.tsx', '.js'],
        alias: {
            // HACK: This is required because the codeintellify package has a hardcoded import that assumes that
            // ../node_modules/@sourcegraph/react-loading-spinner is a valid path. This is not a correct assumption
            // in general, and it also breaks in this build because CSS imports URLs are not resolved (we would
            // need to use resolve-url-loader). There are many possible fixes that are more complex, but this hack
            // works fine for now.
            '../node_modules/@sourcegraph/react-loading-spinner/lib/LoadingSpinner.css': require.resolve(
                '@sourcegraph/react-loading-spinner/lib/LoadingSpinner.css'
            ),
        },
    },
    module: {
        rules: [
            tsRule,
            jsRule,
            {
                // SCSS rule for our own styles and Bootstrap
                test: /\.(css|sass|scss)$/,
                use: [
                    MiniCssExtractPlugin.loader,
                    {
                        loader: 'css-loader',
                    },
                    ...commonStylesheetLoaders,
                ],
            },
        ],
    },
}
export default config
