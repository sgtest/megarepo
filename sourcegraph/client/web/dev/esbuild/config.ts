import path from 'path'

import { sentryEsbuildPlugin } from '@sentry/esbuild-plugin'
import type * as esbuild from 'esbuild'

import { ROOT_PATH, STATIC_ASSETS_PATH } from '@sourcegraph/build-config'
import {
    stylePlugin,
    packageResolutionPlugin,
    monacoPlugin,
    RXJS_RESOLUTIONS,
    buildTimerPlugin,
} from '@sourcegraph/build-config/src/esbuild/plugins'
import { MONACO_LANGUAGES_AND_FEATURES } from '@sourcegraph/build-config/src/monaco-editor'

import type { EnvironmentConfig } from '../utils'

import { manifestPlugin } from './manifestPlugin'

/**
 * Creates esbuild build options for the client/web app.
 */
export function esbuildBuildOptions(ENVIRONMENT_CONFIG: EnvironmentConfig): esbuild.BuildOptions {
    return {
        entryPoints: ENVIRONMENT_CONFIG.CODY_APP
            ? [path.join(ROOT_PATH, 'client/web/src/enterprise/app/main.tsx')]
            : [
                  path.join(ROOT_PATH, 'client/web/src/enterprise/main.tsx'),
                  path.join(ROOT_PATH, 'client/web/src/enterprise/embed/embedMain.tsx'),
              ],
        bundle: true,
        minify: ENVIRONMENT_CONFIG.NODE_ENV === 'production',
        treeShaking: true,

        format: 'esm',
        logLevel: 'error',
        jsx: 'automatic',
        jsxDev: ENVIRONMENT_CONFIG.NODE_ENV === 'development',
        splitting: !ENVIRONMENT_CONFIG.DEV_WEB_BUILDER_NO_SPLITTING,
        chunkNames: 'chunks/chunk-[name]-[hash]',
        entryNames: '[name]-[hash]',
        outdir: STATIC_ASSETS_PATH,
        plugins: [
            stylePlugin,
            manifestPlugin,
            packageResolutionPlugin({
                path: require.resolve('path-browserify'),
                ...RXJS_RESOLUTIONS,
                ...(ENVIRONMENT_CONFIG.DEV_WEB_BUILDER_OMIT_SLOW_DEPS
                    ? {
                          // Monaco
                          '@sourcegraph/shared/src/components/MonacoEditor':
                              '@sourcegraph/shared/src/components/NoMonacoEditor',
                          'monaco-editor': '/dev/null',
                          'monaco-editor/esm/vs/editor/editor.api': '/dev/null',
                          'monaco-yaml': '/dev/null',

                          // GraphiQL
                          './api/ApiConsole': path.join(ROOT_PATH, 'client/web/src/api/NoApiConsole.tsx'),
                          '@graphiql/react': '/dev/null',
                          graphiql: '/dev/null',

                          // Misc.
                          recharts: '/dev/null',
                      }
                    : null),
            }),
            ENVIRONMENT_CONFIG.DEV_WEB_BUILDER_OMIT_SLOW_DEPS ? null : monacoPlugin(MONACO_LANGUAGES_AND_FEATURES),
            buildTimerPlugin,
            ENVIRONMENT_CONFIG.SENTRY_UPLOAD_SOURCE_MAPS
                ? sentryEsbuildPlugin({
                      org: ENVIRONMENT_CONFIG.SENTRY_ORGANIZATION,
                      project: ENVIRONMENT_CONFIG.SENTRY_PROJECT,
                      authToken: ENVIRONMENT_CONFIG.SENTRY_DOT_COM_AUTH_TOKEN,
                      silent: true,
                      release: { name: `frontend@${ENVIRONMENT_CONFIG.VERSION}` },
                      sourcemaps: { assets: [path.join('dist', '*.map'), path.join('dist', 'chunks', '*.map')] },
                  })
                : null,
        ].filter((plugin): plugin is esbuild.Plugin => plugin !== null),
        define: {
            ...Object.fromEntries(
                Object.entries({ ...ENVIRONMENT_CONFIG, SOURCEGRAPH_API_URL: undefined }).map(([key, value]) => [
                    `process.env.${key}`,
                    JSON.stringify(value === undefined ? null : value),
                ])
            ),
            global: 'window',
        },
        loader: {
            '.yaml': 'text',
            '.ttf': 'file',
            '.woff2': 'file',
            '.png': 'file',
        },
        target: 'esnext',
        sourcemap: true,
    }
}
