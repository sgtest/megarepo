import * as fs from 'mz/fs'
import * as path from 'path'
import rmfr from 'rmfr'
import { ConnectionCache, DocumentCache, ResultChunkCache } from './cache'
import { convertLsif } from './importer'
import { createCommit, createLocation, getTestData } from './test-utils'
import { createDatabaseFilename } from './util'
import { Database } from './database'
import { XrepoDatabase } from './xrepo'

describe('Database', () => {
    let storageRoot!: string
    const repository = 'test'
    const commit = createCommit('test')

    const connectionCache = new ConnectionCache(10)
    const documentCache = new DocumentCache(10)
    const resultChunkCache = new ResultChunkCache(10)

    beforeAll(async () => {
        storageRoot = await fs.promises.mkdtemp('typescript-')
        const xrepoDatabase = new XrepoDatabase(connectionCache, path.join(storageRoot, 'xrepo.db'))

        const input = await getTestData('typescript/linked-reference-results/data/data.lsif.gz')
        const database = createDatabaseFilename(storageRoot, repository, commit)
        const { packages, references } = await convertLsif(input, database)
        await xrepoDatabase.addPackagesAndReferences(repository, commit, packages, references)
    })

    afterAll(async () => await rmfr(storageRoot))

    const loadDatabase = (repository: string, commit: string): Database =>
        new Database(
            storageRoot,
            new XrepoDatabase(connectionCache, path.join(storageRoot, 'xrepo.db')),
            connectionCache,
            documentCache,
            resultChunkCache,
            repository,
            commit,
            createDatabaseFilename(storageRoot, repository, commit)
        )

    it('should find all refs of `foo`', async () => {
        const db = loadDatabase(repository, commit)

        const positions = [
            { line: 1, character: 5 },
            { line: 5, character: 5 },
            { line: 9, character: 5 },
            { line: 13, character: 3 },
            { line: 16, character: 3 },
        ]

        for (const position of positions) {
            const references = await db.references('src/index.ts', position)
            expect(references).toContainEqual(createLocation('src/index.ts', 1, 4, 1, 7)) // abstract def in I
            expect(references).toContainEqual(createLocation('src/index.ts', 5, 4, 5, 7)) // concrete def in A
            expect(references).toContainEqual(createLocation('src/index.ts', 9, 4, 9, 7)) // concrete def in B
            expect(references).toContainEqual(createLocation('src/index.ts', 13, 2, 13, 5)) // use via I
            expect(references).toContainEqual(createLocation('src/index.ts', 16, 2, 16, 5)) // use via B

            // Ensure no additional references
            expect(references && references.length).toEqual(5)
        }
    })
})
