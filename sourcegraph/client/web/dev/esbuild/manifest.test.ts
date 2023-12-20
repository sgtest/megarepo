import { expect, describe, test } from 'vitest'

import { type WebBuildManifest, createManifestFromBuildResult } from './manifest'

describe('createManifestFromBuildResult', () => {
    test('non-bazel', () =>
        expect(
            createManifestFromBuildResult(
                { entryPoints: ['src/enterprise/main.tsx', 'src/enterprise/embed/embedMain.tsx'], outdir: 'dist' },
                {
                    'dist/main-AAA.js': {
                        entryPoint: 'src/enterprise/main.tsx',
                        cssBundle: 'dist/main-BBB.css',
                        imports: [],
                        exports: [],
                        inputs: {},
                        bytes: 123,
                    },
                    'dist/embedMain-CCC.js': {
                        entryPoint: 'src/enterprise/embed/embedMain.tsx',
                        cssBundle: 'dist/embedMain-DDD.css',
                        imports: [],
                        exports: [],
                        inputs: {},
                        bytes: 123,
                    },
                }
            )
        ).toEqual({
            'main.js': 'main-AAA.js',
            'main.css': 'main-BBB.css',
            'embed.js': 'embedMain-CCC.js',
            'embed.css': 'embedMain-DDD.css',
        } satisfies WebBuildManifest))

    test('bazel', () =>
        expect(
            createManifestFromBuildResult(
                { entryPoints: ['src/enterprise/main.js', 'src/enterprise/embed/embedMain.js'], outdir: 'dist' },
                {
                    'client/web/bundle/main-AAA.js': {
                        entryPoint: 'src/enterprise/main.js',
                        cssBundle: 'client/web/bundle/main-BBB.css',
                        imports: [],
                        exports: [],
                        inputs: {},
                        bytes: 123,
                    },
                    'client/web/bundle/embedMain-CCC.js': {
                        entryPoint: 'src/enterprise/embed/embedMain.js',
                        cssBundle: 'client/web/bundle/embedMain-DDD.css',
                        imports: [],
                        exports: [],
                        inputs: {},
                        bytes: 123,
                    },
                },
                true
            )
        ).toEqual({
            'main.js': 'main-AAA.js',
            'main.css': 'main-BBB.css',
            'embed.js': 'embedMain-CCC.js',
            'embed.css': 'embedMain-DDD.css',
        } satisfies WebBuildManifest))
})
