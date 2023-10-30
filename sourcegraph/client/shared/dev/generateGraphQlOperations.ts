import { readFileSync } from 'fs'
import path from 'path'

import { CodegenConfig, generate } from '@graphql-codegen/cli'
import { glob } from 'glob'
import { GraphQLError } from 'graphql'

const ROOT_FOLDER = path.resolve(__dirname, '../../../')

const WEB_FOLDER = path.resolve(ROOT_FOLDER, './client/web')
const SVELTEKIT_FOLDER = path.resolve(ROOT_FOLDER, './client/web-sveltekit')
const BROWSER_FOLDER = path.resolve(ROOT_FOLDER, './client/browser')
const SHARED_FOLDER = path.resolve(ROOT_FOLDER, './client/shared')
const VSCODE_FOLDER = path.resolve(ROOT_FOLDER, './client/vscode')
const JETBRAINS_FOLDER = path.resolve(ROOT_FOLDER, './client/jetbrains')
const SCHEMA_PATH = path.join(ROOT_FOLDER, './cmd/frontend/graphqlbackend/*.graphql')

const SHARED_DOCUMENTS_GLOB = [`${SHARED_FOLDER}/src/**/*.{ts,tsx}`]

const WEB_DOCUMENTS_GLOB = [
    `${WEB_FOLDER}/src/**/*.{ts,tsx}`,
    `${WEB_FOLDER}/src/regression/**/*.*`,
    `!${WEB_FOLDER}/src/end-to-end/**/*.*`, // TODO(bazel): can remove when non-bazel dropped
]

const SVELTEKIT_DOCUMENTS_GLOB = [`${SVELTEKIT_FOLDER}/src/lib/**/*.ts`]

const BROWSER_DOCUMENTS_GLOB = [
    `${BROWSER_FOLDER}/src/**/*.{ts,tsx}`,
    `!${BROWSER_FOLDER}/src/end-to-end/**/*.*`, // TODO(bazel): can remove when non-bazel dropped
    '!**/*.d.ts',
]

const VSCODE_DOCUMENTS_GLOB = [`${VSCODE_FOLDER}/src/**/*.{ts,tsx}`]

const JETBRAINS_DOCUMENTS_GLOB = [`${JETBRAINS_FOLDER}/webview/src/**/*.{ts,tsx}`]

const GLOBS: Record<string, string[]> = {
    BrowserGraphQlOperations: BROWSER_DOCUMENTS_GLOB,
    JetBrainsGraphQlOperations: JETBRAINS_DOCUMENTS_GLOB,
    SharedGraphQlOperations: SHARED_DOCUMENTS_GLOB,
    VSCodeGraphQlOperations: VSCODE_DOCUMENTS_GLOB,
    WebGraphQlOperations: WEB_DOCUMENTS_GLOB,
    SvelteKitGraphQlOperations: SVELTEKIT_DOCUMENTS_GLOB,
}

const EXTRA_PLUGINS: Record<string, string[]> = {
    SharedGraphQlOperations: ['typescript-apollo-client-helpers'],
}

const SHARED_PLUGINS = [
    `${SHARED_FOLDER}/dev/extractGraphQlOperationCodegenPlugin.js`,
    'typescript',
    'typescript-operations',
]
interface Input {
    interfaceNameForOperations: string
    outputPath: string
}

export const ALL_INPUTS: Input[] = [
    {
        interfaceNameForOperations: 'BrowserGraphQlOperations',
        outputPath: path.join(BROWSER_FOLDER, './src/graphql-operations.ts'),
    },
    {
        interfaceNameForOperations: 'WebGraphQlOperations',
        outputPath: path.join(WEB_FOLDER, './src/graphql-operations.ts'),
    },
    {
        interfaceNameForOperations: 'SvelteKitGraphQlOperations',
        outputPath: path.join(SVELTEKIT_FOLDER, './src/lib/graphql-operations.ts'),
    },
    {
        interfaceNameForOperations: 'SharedGraphQlOperations',
        outputPath: path.join(SHARED_FOLDER, './src/graphql-operations.ts'),
    },
    {
        interfaceNameForOperations: 'VSCodeGraphQlOperations',
        outputPath: path.join(VSCODE_FOLDER, './src/graphql-operations.ts'),
    },
    {
        interfaceNameForOperations: 'JetBrainsGraphQlOperations',
        outputPath: path.join(JETBRAINS_FOLDER, './webview/src/graphql-operations.ts'),
    },
]

/**
 * Resolve the globs to files and filter to only files containing "gql`" (which indicates that they
 * contain a GraphQL operation). The @graphql-codegen/typescript plugin does more advanced filtering
 * using an AST parse tree, but this simple string check skips AST parsing and saves a lot of time.
 */
function resolveAndFilterGlobs(globs: string[]): string[] {
    const files = globs.flatMap(p =>
        p.startsWith('!') ? p : glob.sync(p).filter(file => readFileSync(file, 'utf-8').includes('gql`'))
    )
    return files
}

export function createCodegenConfig(operations: Input[]): CodegenConfig {
    const generates: CodegenConfig['generates'] = {}
    for (const operation of operations) {
        generates[operation.outputPath] = {
            documents: resolveAndFilterGlobs(GLOBS[operation.interfaceNameForOperations]),
            config: {
                onlyOperationTypes: true,
                noExport: false,
                enumValues:
                    operation.interfaceNameForOperations === 'SharedGraphQlOperations'
                        ? undefined
                        : '@sourcegraph/shared/src/graphql-operations',
                interfaceNameForOperations: operation.interfaceNameForOperations,
            },
            plugins: [...SHARED_PLUGINS, ...(EXTRA_PLUGINS[operation.interfaceNameForOperations] || [])],
        }
    }

    return {
        schema: SCHEMA_PATH,
        errorsOnly: true,
        silent: true,
        config: {
            // https://the-guild.dev/graphql/codegen/plugins/typescript/typescript-operations#config-api-reference
            arrayInputCoercion: false,
            preResolveTypes: true,
            operationResultSuffix: 'Result',
            omitOperationSuffix: true,
            namingConvention: {
                typeNames: 'keep',
                enumValues: 'keep',
                transformUnderscore: true,
            },
            declarationKind: 'interface',
            avoidOptionals: {
                field: true,
                inputValue: false,
                object: true,
            },
            scalars: {
                DateTime: 'string',
                JSON: 'object',
                JSONValue: 'unknown',
                GitObjectID: 'string',
                JSONCString: 'string',
                PublishedValue: "boolean | 'draft'",
                BigInt: 'string',
            },
        },
        generates,
    }
}

if (require.main === module) {
    // Entry point to generate all GraphQL operations files, or a single one.
    async function main(args: string[]) {
        if (args.length !== 0 && args.length !== 2) {
            throw new Error('Usage: [<schemaName> <outputPath>]')
        }
        await generate(
            createCodegenConfig(
                args.length === 0 ? ALL_INPUTS : [{ interfaceNameForOperations: args[0], outputPath: args[1] }]
            )
        )
    }
    main(process.argv.slice(2)).catch(error => {
        console.error(error)
        if (error instanceof AggregateError) {
            for (const e of error.errors) {
                if (e instanceof GraphQLError) {
                    console.error(e.source, e.locations)
                }
            }
        }
        process.exit(1)
    })
}
