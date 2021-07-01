import { spawnSync } from 'child_process'
import fs from 'fs'
import path from 'path'

import signale from 'signale'

import { dllPluginConfig, storybookWorkspacePath, rootPath } from '../webpack.config.common'

// Build DLL bundle with `yarn build:dll-bundle` if it's not available.
export const ensureDllBundleIsReady = (): void => {
    signale.start(`Checking if DLL bundle is available: ${path.relative(rootPath, dllPluginConfig.path)}`)

    // eslint-disable-next-line no-sync
    if (!fs.existsSync(dllPluginConfig.path)) {
        signale.warn('DLL bundle not found!')
        signale.await('Building new DLL bundle with `yarn build:dll-bundle`')

        spawnSync('yarn', ['build:dll-bundle'], {
            stdio: 'inherit',
            cwd: storybookWorkspacePath,
        })
    }

    signale.success('DLL bundle is ready!')
}
