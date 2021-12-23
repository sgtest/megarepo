import path from 'path'

import webpack from 'webpack'

import { ROOT_PATH } from '../paths'

interface CacheConfigOptions {
    invalidateCacheFiles: string[]
}

export const getCacheConfig = ({ invalidateCacheFiles }: CacheConfigOptions): webpack.Configuration['cache'] => ({
    type: 'filesystem',
    buildDependencies: {
        // Invalidate cache on config change.
        config: [
            ...invalidateCacheFiles,
            path.resolve(ROOT_PATH, 'babel.config.js'),
            path.resolve(ROOT_PATH, 'postcss.config.js'),
        ],
    },
})
