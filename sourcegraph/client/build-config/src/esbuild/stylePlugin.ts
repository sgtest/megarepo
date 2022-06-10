import fs from 'fs'
import path from 'path'

import { ResolverFactory, CachedInputFileSystem } from 'enhanced-resolve'
import esbuild from 'esbuild'
import postcss from 'postcss'
import postcssModules from 'postcss-modules'
import sass from 'sass'

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import postcssConfig from '../../../../postcss.config'
import { NODE_MODULES_PATH, ROOT_PATH } from '../paths'

/**
 * An esbuild plugin that builds .css and .scss stylesheets (including support for CSS modules).
 */
export const stylePlugin: esbuild.Plugin = {
    name: 'style',
    setup: build => {
        const modulesMap = new Map<string, string>()
        const modulesPlugin = postcssModules({
            generateScopedName: '[name]__[local]', // omit hash for local dev
            localsConvention: 'camelCase',
            getJSON: (cssPath, json) => modulesMap.set(cssPath, JSON.stringify(json)),
        })

        interface TransformArguments {
            inputPath: string
            inputContents: string
        }
        interface TransformResult {
            outputPath: string
            outputContents: string
            includedFiles: string[]
            mtime: number
        }
        const transform = async ({ inputPath, inputContents }: TransformArguments): Promise<TransformResult> => {
            const isSCSS = inputPath.endsWith('.scss')
            const sassResult = isSCSS
                ? // renderSync is ~20% faster than render with an async callback (because it's blocked on CPU, not IO).
                  // eslint-disable-next-line no-sync
                  sass.renderSync({
                      file: inputPath,
                      data: inputContents,
                      includePaths: [path.resolve(ROOT_PATH, 'node_modules'), path.resolve(ROOT_PATH, 'client')],
                  })
                : null

            const css = sassResult?.css.toString() ?? inputContents
            const includedFiles = sassResult?.stats.includedFiles.filter(value => typeof value === 'string') ?? []

            const outputPath = isSCSS ? inputPath.replace(/\.scss$/, '.css') : inputPath

            const isCSSModule = outputPath.endsWith('.module.css')
            const result = await postcss(
                // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
                isCSSModule ? [...postcssConfig.plugins, modulesPlugin] : postcssConfig.plugins
            ).process(css, {
                from: outputPath,
            })
            return {
                outputPath,
                outputContents: result.css,
                includedFiles,
                mtime: Date.now(),
            }
        }
        const transformCache = new Map<
            TransformArguments['inputPath'],
            Pick<TransformArguments, 'inputContents'> & TransformResult
        >()
        const cachedTransform = async ({ inputPath, inputContents }: TransformArguments): Promise<TransformResult> => {
            const cached = transformCache.get(inputPath)
            // If the input file has changed, or any included file has changed, then the cache entry is stale.
            if (cached && cached.inputContents === inputContents) {
                let allInputFilesFresh = true
                for (const path of cached.includedFiles) {
                    try {
                        const stat = await fs.promises.stat(path)
                        if (stat.mtimeMs > cached.mtime) {
                            allInputFilesFresh = false
                            break // included file has changed since last build
                        }
                    } catch {
                        // Included file was (likely) deleted (or otherwise made inaccessible) since last build.
                        allInputFilesFresh = false
                        break
                    }
                }
                if (allInputFilesFresh) {
                    return cached
                }
            }

            const output = await transform({ inputPath, inputContents })
            transformCache.set(inputPath, { inputContents, ...output })
            return output
        }

        const resolver = ResolverFactory.createResolver({
            fileSystem: new CachedInputFileSystem(fs, 4000),
            extensions: ['.css', '.scss'],
            symlinks: true, // Resolve workspace symlinks
            modules: [NODE_MODULES_PATH],
        })

        build.onResolve({ filter: /\.s?css$/, namespace: 'file' }, async args => {
            const inputPath = await new Promise<string>((resolve, reject) => {
                resolver.resolve({}, args.resolveDir, args.path, {}, (error, filepath) => {
                    if (filepath) {
                        resolve(filepath)
                    } else {
                        reject(error ?? new Error(`Could not resolve file path for ${args.path}`))
                    }
                })
            })

            const { outputPath, outputContents, includedFiles } = await cachedTransform({
                inputPath,
                inputContents: await fs.promises.readFile(inputPath, 'utf8'),
            })
            const isCSSModule = outputPath.endsWith('.module.css')
            return {
                path: outputPath,
                namespace: isCSSModule ? 'css-module' : 'css',
                pluginData: { contents: outputContents },
                watchFiles: includedFiles,
            }
        })

        // Resolve CSS modules imported by the next onLoad callback to the actual stylesheet (not
        // the synthesized JavaScript module that exports the CSS module's class names).
        build.onResolve({ filter: /\.css$/, namespace: 'css-module' }, args => ({
            path: args.path,
            namespace: 'css',
            // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
            pluginData: { contents: args.pluginData?.contents },
        }))

        // Load a synthesized JavaScript module that exports the CSS module's class names and
        // imports (for side effects) the actual CSS file.
        build.onLoad({ filter: /\.module\.css$/, namespace: 'css-module' }, args => ({
            contents: `
import ${JSON.stringify(args.path)}
export default ${modulesMap.get(args.path) || '{}'}`,
            loader: 'js',
            resolveDir: path.dirname(args.path),
            // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
            pluginData: args.pluginData,
        }))

        // Load the contents of all CSS files. The transformed CSS was passed through `pluginData.contents`.
        build.onLoad({ filter: /\.css$/, namespace: 'css' }, args => ({
            // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
            contents: args.pluginData?.contents,
            resolveDir: path.dirname(args.path),
            loader: 'css',
        }))
    },
}
