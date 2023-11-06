import path from 'path'

import { type UserConfig, type UserWorkspaceConfig, defineProject, mergeConfig } from 'vitest/config'

/** Whether we're running in Bazel. */
export const BAZEL = !!process.env.BAZEL

/** Use .js extension when running in Bazel. */
const TS_EXT = BAZEL ? 'js' : 'ts'

/**
 * Default configuration for a project in a workspace.
 */
export const defaultProjectConfig: UserWorkspaceConfig = {
    logLevel: 'warn',
    clearScreen: false,
    test: {
        testTimeout: 5000,
        hookTimeout: 1000,
        experimentalVmThreads: true,
        include: [`**/*.test.${TS_EXT}?(x)`],
        exclude: [
            '**/integration-test',
            '**/integration',
            '**/end-to-end',
            '**/regression',
            '**/node_modules',
            '**/dist',
            '**/out',
            '.git',
            '**/.cache',
            '**/*.runfiles/__main__', // for Bazel
            '**/testSetup.test.[jt]s',
        ],
        css: { modules: { classNameStrategy: 'non-scoped' } },
        hideSkippedTests: true,
        globalSetup: [path.join(__dirname, `client/testing/src/globalTestSetup.${TS_EXT}`)],
    },
    plugins: BAZEL
        ? [
              {
                  // In Bazel, `.module.scss` files have already been built to `.module.css` files,
                  // so we need to import those instead.
                  name: 'import-module-css-not-scss',
                  enforce: 'post',
                  resolveId(id, importer) {
                      if (id.endsWith('.module.scss')) {
                          return { id: path.join(importer ?? '', '..', id.replace('.module.scss', '.module.css')) }
                      }
                      return undefined
                  },
              },
          ]
        : undefined,
}

/**
 * Configuration that applies to the entire workspace.
 */
const userConfig: UserConfig = {
    test: {
        cache: BAZEL ? false : undefined, // don't cache in Bazel

        minThreads: 1, // otherwise it's slow when there are many CPU cores
        maxThreads: 16,
        teardownTimeout: 1000,

        // For compatibility with Jest's defaults; can be changed to the Vitest defaults.
        snapshotFormat: {
            escapeString: true,
            printBasicPrototype: true,
        },

        reporters: ['basic'],

        resolveSnapshotPath: BAZEL ? bazelResolveSnapshotPath : undefined,
    },
}

export function defineProjectWithDefaults(dir: string, config: UserWorkspaceConfig): UserWorkspaceConfig {
    const name = path.basename(dir)
    if (!config.test) {
        config.test = {}
    }
    if (!config.test.name) {
        config.test.name = name
    }
    if (!config.test.root) {
        config.test.root = dir
    }

    // Use .js extensions when running in Bazel.
    if (BAZEL) {
        if (config.test.setupFiles) {
            config.test.setupFiles = (
                Array.isArray(config.test.setupFiles) ? config.test.setupFiles : [config.test.setupFiles]
            ).map(toJSExtension)
        }
        if (config.test.environmentMatchGlobs) {
            config.test.environmentMatchGlobs = config.test.environmentMatchGlobs.map(([glob, env]) => [
                toJSExtension(glob),
                env,
            ])
        }
    }

    return mergeConfig(mergeConfig(defaultProjectConfig, userConfig), defineProject(config) as UserWorkspaceConfig)
}

function toJSExtension(path: string): string {
    // TODO(sqs): Because Bazel ts_project preserve_jsx seems to be broken and always transpiles
    // `.tsx` to `.js`, we need to assume that `.tsx` maps to `.js` here. This means that the glob
    // pattern will match more than expected (e.g., if the glob pattern `*.tsx` is rewritten here to
    // `*.js`, then it would match input files with both `.ts` and `.tsx` extensions). This
    // introduces disparity between running `vitest` manually and running it in Bazel, but that is
    // probably OK because the "worst" that would happen is that the jsdom environment would be used
    // where it was not needed. If a file needs to explicitly opt into another environment, it
    // should use the `@vitest-environment` directive at the top.
    return path.replace(/\.ts$/, '.js').replace(/\.tsx$/, '.js')
}

function bazelResolveSnapshotPath(testPath: string, snapshotExtension: string): string {
    // TODO(bazel): drop when non-bazel removed.

    // TODO(bazel): bazel runs tests on the pre-compiled .js files, non-bazel runs on .tsx files.
    // This snapshot resolver edits the pre-compiled .js to snapshots assuming a .tsx extension.
    // This can be removed and snapshot files renamed to .js when non-bazel is removed.
    // NOTE: this assumes all snapshot tests are in .tsx files, not .ts or .jsx and will not work for non-.tsx files.

    /**
     * Bazel runs the tests on pre-compiled .js files so we have no way of knowing
     * if the original test was .ts or .tsx. Vitest requires mapping from the test file
     * name (.js in bazel, .ts[x] in non-bazel) to the snapshot file name (.ts[x].snap)
     * without any additional information - if we only know the .js name we don't know
     * if the snapshot is .ts or .tsx.
     *
     * While we need to support non-bazel we can't update the existing snapshots to .js.snap.
     * For now, we require all snapshot tests use .tsx extensions.
     */
    return path.join(
        path.dirname(testPath),
        '__snapshots__',
        path.basename(testPath).replace('.js', '.tsx') + snapshotExtension
    )
}
