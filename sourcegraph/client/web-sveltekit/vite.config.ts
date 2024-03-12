import { join } from 'path'

import { sveltekit } from '@sveltejs/kit/vite'
import { defineConfig, mergeConfig, type UserConfig } from 'vite'
import inspect from 'vite-plugin-inspect'

import graphqlCodegen from './dev/vite-graphql-codegen'

export default defineConfig(({ mode }) => {
    let config: UserConfig = {
        plugins: [
            sveltekit(),
            // Generates typescript types for gql-tags and .gql files
            graphqlCodegen(),
            inspect(),
        ],
        define:
            mode === 'test'
                ? {}
                : {
                      'process.platform': '"browser"',
                      'process.env.VITEST': 'null',
                      'process.env.NODE_ENV': `"${mode}"`,
                      'process.env': '{}',
                  },
        css: {
            preprocessorOptions: {
                scss: {
                    includePaths: [
                        // Our scss files and scss files in client/* often import global styles via @import 'wildcard/src/...'
                        // Adding '..' as load path causes scss to look for these imports in the client folder.
                        // (without it scss @import paths are always relative to the importing file)
                        join(__dirname, '..'),
                    ],
                },
            },
            modules: {
                localsConvention: 'camelCase',
            },
        },
        server: {
            // Allow setting the port via env variables to make it easier to integrate with
            // our existing caddy setup (which proxies requests to a specific port).
            port: process.env.SK_PORT ? +process.env.SK_PORT : undefined,
            strictPort: !!process.env.SV_PORT,
            proxy: {
                // Proxy requests to specific endpoints to a real Sourcegraph
                // instance.
                '^(/sign-in|/.assets|/-|/.api|/search/stream|/users|/notebooks|/insights)': {
                    target: process.env.SOURCEGRAPH_API_URL || 'https://sourcegraph.com',
                    changeOrigin: true,
                    secure: false,
                },
            },
        },

        resolve: {
            alias: [
                // Unclear why Vite fails. It claims that index.esm.js doesn't have this export (it does).
                // Rewriting this to index.js fixes the issue. Error:
                // import { CiWarning, CiSettings, CiTextAlignLeft } from "react-icons/ci/index.esm.js";
                //                     ^^^^^^^^^^
                // SyntaxError: Named export 'CiSettings' not found. The requested module 'react-icons/ci/index.esm.js'
                // is a CommonJS module, which may not support all module.exports as named exports.
                {
                    find: /^react-icons\/(.+)$/,
                    replacement: 'react-icons/$1/index.js',
                },
                // We generate corresponding .gql.ts files for .gql files.
                // This alias allows us to import .gql files and have them resolved to the generated .gql.ts files.
                {
                    find: /^(.*)\.gql$/,
                    replacement: '$1.gql.ts',
                },
                // In rxjs v6 these are directories and cannot be imported from directly in the production build.
                // The following error occurs:
                // Error [ERR_UNSUPPORTED_DIR_IMPORT]: Directory import '[...]/node_modules/rxjs/operators' is not supported resolving ES modules
                {
                    find: /^rxjs\/(operators|fetch)$/,
                    replacement: 'rxjs/$1/index.js',
                    customResolver(source, importer, options) {
                        // This is an hacky way to make the dev build work. @sourcegraph/telemetry uses a newer
                        // version of rxjs (v7) where `rjx/operators` and `rxjs/fetch` are properly mapped
                        // to their respective files in package.json.
                        // Applying the same replacement to this version results in an error.
                        // I tried various ways to prevent having the alias be applied to `@sourcegraph/telemetry`
                        // without success:
                        // - Removing this alias causes the production build to fail do the issue mentioned at the
                        //   top of this alias.
                        // - Adding something like `{ ssr: { external: '@sourcegraph/telemetry' } }` to the config
                        //   does not prevent the alias from being applied. Maybe I don't understand how `external`
                        //   is supposed to work.
                        // - Using a custom plugin that implements a custom resolveId function is somehow not being
                        //   run in the production build for `rxjs` imports. Maybe it has something to do with the
                        //   interop between vite and sveltekit.
                        if (importer?.includes('@sourcegraph/telemetry')) {
                            source = source.replace('/index.js', '')
                        }
                        return this.resolve(source, importer, options)
                    },
                },
                // Without aliasing lodash to lodash-es we get the following error:
                // SyntaxError: Named export 'castArray' not found. The requested module 'lodash' is a CommonJS module, which may not support all module.exports as named exports.
                {
                    find: /^lodash$/,
                    replacement: 'lodash-es',
                },
            ],
        },

        optimizeDeps: {
            exclude: [
                // Without addings this Vite throws an error
                'linguist-languages',
            ],
        },

        test: {
            setupFiles: './src/testing/setup.ts',
            include: ['src/**/*.test.ts'],
        },

        legacy: {
            // Our existing codebase imports many CommonJS modules as if they were ES modules. The default
            // Vite 5 behavior doesn't work with this. Enabling this should be OK since we don't
            // actually use SSR at the moment, so the difference between the dev and prod builds don't matter.
            // We should revisit this at some point though.
            // See https://vitejs.dev/guide/migration.html#ssr-externalized-modules-value-now-matches-production
            proxySsrExternalModules: true,
        },
    }

    if (process.env.BAZEL) {
        // Merge settings necessary to make the build work with bazel
        config = mergeConfig(config, {
            resolve: {
                alias: [
                    // When using Bazel, @sourcegraph/* dependencies will refer to the built packages.
                    // These do not contain the source *.module.scss files but still contain import statements
                    // that reference *.scss files. Processing them with vite throws an error unless we
                    // update the imports to reference the corresponding *.css files instead.
                    // Additionally our own source files might reference *.module.scss files, which we also want
                    // to rewrite.
                    {
                        find: /^(.+)\.module\.scss$/,
                        replacement: '$1.module.css',
                        customResolver(source, importer, options) {
                            // The this.resolve(...) part is taken from the @rollup/plugin-alias implementation. Without
                            // it it appears the bundler tries to resolve relative module IDs to the current working
                            // directory.
                            return source.includes('@sourcegraph') || importer?.includes('@sourcegraph/')
                                ? this.resolve(source, importer, { skipSelf: true, ...options }).then(
                                      resolved => resolved || { id: source }
                                  )
                                : null
                        },
                    },
                ],
            },
            ssr: {
                // By default vite treats dependencies that are links to other packages in the monorepo as source code
                // and processes them as well.
                // In a bazel sandbox however all @sourcegraph/* dependencies are built packages and thus not processed
                // by vite without this additional setting.
                // We have to process those files to apply certain "fixes", such as aliases defined in here
                // and in svelte.config.js.
                noExternal: [/@sourcegraph\/.*/],
                // Exceptions to the above rule. These are packages that are not part of this monorepo and should
                // not be processed by vite.
                external: ['@sourcegraph/telemetry'],
            },
        } satisfies UserConfig)
    }

    return config
})
