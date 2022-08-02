import { isExternalLink } from '@sourcegraph/common'

import { SearchPatternType } from '../graphql-operations'

import {
    buildSearchURLQuery,
    makeRepoURI,
    parseHash,
    parseRepoURI,
    toPrettyBlobURL,
    withWorkspaceRootInputRevision,
    toAbsoluteBlobURL,
    RepoFile,
    toRepoURL,
} from './url'

/**
 * Asserts deep object equality using node's assert.deepEqual, except it (1) ignores differences in the
 * prototype (because that causes 2 object literals to fail the test) and (2) treats undefined properties as
 * missing.
 */
function assertDeepStrictEqual(actual: any, expected: any): void {
    actual = JSON.parse(JSON.stringify(actual))
    expected = JSON.parse(JSON.stringify(expected))
    expect(actual).toEqual(expected)
}

describe('parseRepoURI', () => {
    test('should parse repo', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
        })
    })
    test('should parse repo with spaces', () => {
        const parsed = parseRepoURI('git://sourcegraph.visualstudio.com/Test%20Repo')
        assertDeepStrictEqual(parsed, {
            repoName: 'sourcegraph.visualstudio.com/Test Repo',
        })
    })

    test('should parse repo with plus sign', () => {
        const parsed = parseRepoURI('git://git.launchpad.net/ubuntu/+source/qemu')
        assertDeepStrictEqual(parsed, {
            repoName: 'git.launchpad.net/ubuntu/+source/qemu',
        })
    })

    test('should parse repo with revision', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
        })
    })

    test('should parse repo with revision with special characters', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?my%2Fbranch')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'my/branch',
        })
    })

    test('should parse repo with commitID', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?24fca303ac6da784b9e8269f724ddeb0b2eea5e7')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: '24fca303ac6da784b9e8269f724ddeb0b2eea5e7',
            commitID: '24fca303ac6da784b9e8269f724ddeb0b2eea5e7',
        })
    })

    test('should parse repo with revision and file', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch#mux.go')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
        })
    })

    test('should parse repo with revision and file with spaces', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch#my%20file.go')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'my file.go',
        })
    })

    test('should parse repo with revision and file and line', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch#mux.go:3')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
            position: {
                line: 3,
                character: 0,
            },
        })
    })

    test('should parse repo with revision and file and position', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch#mux.go:3,5')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
            position: {
                line: 3,
                character: 5,
            },
        })
    })

    test('should parse repo with revision and file and range', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch#mux.go:3,5-6,9')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
            range: {
                start: {
                    line: 3,
                    character: 5,
                },
                end: {
                    line: 6,
                    character: 9,
                },
            },
        })
    })

    test('should parse a file with spaces', () => {
        const parsed = parseRepoURI('git://github.com/gorilla/mux?branch#space%20here.go')
        assertDeepStrictEqual(parsed, {
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'space here.go',
        })
    })
})

describe('makeRepoURI', () => {
    test('should make repo', () => {
        const uri = makeRepoURI({
            repoName: 'github.com/gorilla/mux',
        })
        assertDeepStrictEqual(uri, 'git://github.com/gorilla/mux')
    })

    test('should make repo with revision', () => {
        const uri = makeRepoURI({
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
        })
        assertDeepStrictEqual(uri, 'git://github.com/gorilla/mux?branch')
    })

    test('should make repo with commitID', () => {
        const uri = makeRepoURI({
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            commitID: '24fca303ac6da784b9e8269f724ddeb0b2eea5e7',
        })
        assertDeepStrictEqual(uri, 'git://github.com/gorilla/mux?24fca303ac6da784b9e8269f724ddeb0b2eea5e7')
    })

    test('should make repo with revision and file', () => {
        const uri = makeRepoURI({
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
        })
        assertDeepStrictEqual(uri, 'git://github.com/gorilla/mux?branch#mux.go')
    })

    test('should make repo with revision and file and line', () => {
        const uri = makeRepoURI({
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
            position: {
                line: 3,
                character: 0,
            },
        })
        assertDeepStrictEqual(uri, 'git://github.com/gorilla/mux?branch#mux.go:3')
    })

    test('should make repo with revision and file and position', () => {
        const uri = makeRepoURI({
            repoName: 'github.com/gorilla/mux',
            revision: 'branch',
            filePath: 'mux.go',
            position: {
                line: 3,
                character: 5,
            },
        })
        assertDeepStrictEqual(uri, 'git://github.com/gorilla/mux?branch#mux.go:3,5')
    })
})

describe('util/url', () => {
    const linePosition = { line: 1 }
    const lineCharPosition = { line: 1, character: 1 }
    const referenceMode = { ...lineCharPosition, viewState: 'references' }
    const context: RepoFile = {
        repoName: 'github.com/gorilla/mux',
        revision: '',
        commitID: '24fca303ac6da784b9e8269f724ddeb0b2eea5e7',
        filePath: 'mux.go',
    }

    describe('parseHash', () => {
        test('parses empty hash', () => {
            expect(parseHash('')).toEqual({})
        })

        test('parses unexpectedly formatted hash', () => {
            expect(parseHash('L-53')).toEqual({})
            expect(parseHash('L53:')).toEqual({})
            expect(parseHash('L1:2-')).toEqual({})
            expect(parseHash('L1:2-3')).toEqual({})
            expect(parseHash('L1:2-3:')).toEqual({})
            expect(parseHash('L1:-3:')).toEqual({})
            expect(parseHash('L1:-3:4')).toEqual({})
            expect(parseHash('L1-2:3')).toEqual({})
            expect(parseHash('L1-2:')).toEqual({})
            expect(parseHash('L1:-2')).toEqual({})
            expect(parseHash('L1:2--3:4')).toEqual({})
            expect(parseHash('L53:a')).toEqual({})
        })

        test('parses hash with leading octothorpe', () => {
            expect(parseHash('#L1')).toEqual(linePosition)
        })

        test('parses hash with line', () => {
            expect(parseHash('L1')).toEqual(linePosition)
        })

        test('parses hash with line and character', () => {
            expect(parseHash('L1:1')).toEqual(lineCharPosition)
        })

        test('parses hash with range', () => {
            expect(parseHash('L1-2')).toEqual({ line: 1, endLine: 2 })
            expect(parseHash('L1:2-3:4')).toEqual({ line: 1, character: 2, endLine: 3, endCharacter: 4 })
            expect(parseHash('L47-L55')).toEqual({ line: 47, endLine: 55 })
            expect(parseHash('L34:2-L38:3')).toEqual({ line: 34, character: 2, endLine: 38, endCharacter: 3 })
        })

        test('parses hash with references', () => {
            expect(parseHash('$references')).toEqual({ viewState: 'references' })
            expect(parseHash('L1:1$references')).toEqual(referenceMode)
            expect(parseHash('L1:1$references')).toEqual(referenceMode)
        })
        test('parses modern hash with references', () => {
            expect(parseHash('tab=references')).toEqual({ viewState: 'references' })
            expect(parseHash('L1:1&tab=references')).toEqual(referenceMode)
            expect(parseHash('L1:1&tab=references')).toEqual(referenceMode)
        })
    })

    describe('toPrettyBlobURL', () => {
        test('formats url for empty revision', () => {
            expect(toPrettyBlobURL(context)).toBe('/github.com/gorilla/mux/-/blob/mux.go')
        })

        test('formats url for specified revision', () => {
            expect(toPrettyBlobURL({ ...context, revision: 'branch' })).toBe(
                '/github.com/gorilla/mux@branch/-/blob/mux.go'
            )
        })

        test('formats url with position', () => {
            expect(toPrettyBlobURL({ ...context, position: lineCharPosition })).toBe(
                '/github.com/gorilla/mux/-/blob/mux.go?L1:1'
            )
        })

        test('formats url with view state', () => {
            expect(toPrettyBlobURL({ ...context, position: lineCharPosition, viewState: 'references' })).toBe(
                '/github.com/gorilla/mux/-/blob/mux.go?L1:1#tab=references'
            )
        })

        test('formats url with symbols in filePath', () => {
            expect(toPrettyBlobURL({ ...context, filePath: '.shellrc/zshrc.d/functions/gdk.sh##class.Work' })).toBe(
                '/github.com/gorilla/mux/-/blob/.shellrc/zshrc.d/functions/gdk.sh%23%23class.Work'
            )
        })
    })

    describe('toAbsoluteBlobURL', () => {
        const target: RepoFile = {
            repoName: 'github.com/gorilla/mux',
            revision: '',
            commitID: '24fca303ac6da784b9e8269f724ddeb0b2eea5e7',
            filePath: 'mux.go',
        }
        const sourcegraphUrl = 'https://sourcegraph.com'

        test('default sourcegraph URL, default context', () => {
            expect(toAbsoluteBlobURL(sourcegraphUrl, target)).toBe(
                'https://sourcegraph.com/github.com/gorilla/mux/-/blob/mux.go'
            )
        })

        test('default sourcegraph URL, specified revision', () => {
            expect(toAbsoluteBlobURL(sourcegraphUrl, { ...target, revision: 'branch' })).toBe(
                'https://sourcegraph.com/github.com/gorilla/mux@branch/-/blob/mux.go'
            )
        })

        test('default sourcegraph URL, with position', () => {
            expect(toAbsoluteBlobURL(sourcegraphUrl, { ...target, position: { line: 1, character: 1 } })).toBe(
                'https://sourcegraph.com/github.com/gorilla/mux/-/blob/mux.go?L1:1'
            )
        })
    })
})

describe('withWorkspaceRootInputRevision', () => {
    test('uses input revision for URI inside root with input revision', () =>
        expect(
            withWorkspaceRootInputRevision([{ uri: 'git://r?c', inputRevision: 'v' }], parseRepoURI('git://r?c#f'))
        ).toEqual(parseRepoURI('git://r?v#f')))

    test('does not change URI outside root (different repoName)', () =>
        expect(
            withWorkspaceRootInputRevision([{ uri: 'git://r?c', inputRevision: 'v' }], parseRepoURI('git://r2?c#f'))
        ).toEqual(parseRepoURI('git://r2?c#f')))

    test('does not change URI outside root (different revision)', () =>
        expect(
            withWorkspaceRootInputRevision([{ uri: 'git://r?c', inputRevision: 'v' }], parseRepoURI('git://r?c2#f'))
        ).toEqual(parseRepoURI('git://r?c2#f')))

    test('uses empty string input revision (treats differently from undefined)', () =>
        expect(
            withWorkspaceRootInputRevision([{ uri: 'git://r?c', inputRevision: '' }], parseRepoURI('git://r?c#f'))
        ).toEqual({ ...parseRepoURI('git://r?c#f'), revision: '' }))

    test('does not change URI if root has undefined input revision', () =>
        expect(
            withWorkspaceRootInputRevision(
                [{ uri: 'git://r?c', inputRevision: undefined }],
                parseRepoURI('git://r?c#f')
            )
        ).toEqual(parseRepoURI('git://r?c#f')))
})

describe('buildSearchURLQuery', () => {
    it('builds the URL query for a regular expression search', () =>
        expect(buildSearchURLQuery('foo', SearchPatternType.regexp, false, undefined)).toBe('q=foo&patternType=regexp'))
    it('builds the URL query for a literal search', () =>
        expect(buildSearchURLQuery('foo', SearchPatternType.standard, false, undefined)).toBe(
            'q=foo&patternType=standard'
        ))
    it('handles an empty query', () =>
        expect(buildSearchURLQuery('', SearchPatternType.regexp, false, undefined)).toBe('q=&patternType=regexp'))
    it('handles characters that need encoding', () =>
        expect(buildSearchURLQuery('foo bar%baz', SearchPatternType.regexp, false, undefined)).toBe(
            'q=foo+bar%25baz&patternType=regexp'
        ))
    it('preserves / and : for readability', () =>
        expect(buildSearchURLQuery('repo:foo/bar', SearchPatternType.regexp, false, undefined)).toBe(
            'q=repo:foo/bar&patternType=regexp'
        ))
    describe('removal of patternType parameter', () => {
        it('overrides the patternType parameter at the end', () => {
            expect(buildSearchURLQuery('foo patternType:literal', SearchPatternType.regexp, false, undefined)).toBe(
                'q=foo&patternType=literal'
            )
        })
        it('overrides the patternType parameter at the beginning', () => {
            expect(
                buildSearchURLQuery('patternType:literal foo type:diff', SearchPatternType.regexp, false, undefined)
            ).toBe('q=foo+type:diff&patternType=literal')
        })
        it('overrides the patternType parameter at the end with another operator', () => {
            expect(
                buildSearchURLQuery('type:diff foo patternType:literal', SearchPatternType.regexp, false, undefined)
            ).toBe('q=type:diff+foo&patternType=literal')
        })
        it('overrides the patternType parameter in the middle', () => {
            expect(
                buildSearchURLQuery('type:diff patternType:literal foo', SearchPatternType.regexp, false, undefined)
            ).toBe('q=type:diff+foo&patternType=literal')
        })
        it('overrides the patternType parameter if using a quoted value', () => {
            expect(buildSearchURLQuery('patternType:"literal" foo', SearchPatternType.regexp, false, undefined)).toBe(
                'q=foo&patternType=literal'
            )
        })
    })
    it('builds the URL query with a case parameter if caseSensitive is true', () =>
        expect(buildSearchURLQuery('foo', SearchPatternType.standard, true, undefined)).toBe(
            'q=foo&patternType=standard&case=yes'
        ))
    it('appends the case parameter if `case:yes` exists in the query', () =>
        expect(buildSearchURLQuery('foo case:yes', SearchPatternType.standard, false, undefined)).toBe(
            'q=foo+&patternType=standard&case=yes'
        ))
    it('removes the case parameter if using a quoted value', () =>
        expect(buildSearchURLQuery('foo case:"yes"', SearchPatternType.standard, true, undefined)).toBe(
            'q=foo+&patternType=standard&case=yes'
        ))
    it('removes the case parameter case:no exists in the query and caseSensitive is true', () =>
        expect(buildSearchURLQuery('foo case:no', SearchPatternType.standard, true, undefined)).toBe(
            'q=foo+&patternType=standard'
        ))
})

describe('isExternalLink', () => {
    it('returns false for the same site', () => {
        jsdom.reconfigure({ url: 'https://github.com/here' })
        expect(isExternalLink('https://github.com/there')).toBe(false)
    })
    it('returns false for relative links', () => {
        jsdom.reconfigure({ url: 'https://github.com/here' })
        expect(isExternalLink('/there')).toBe(false)
    })
    it('returns false for invalid URLs', () => {
        jsdom.reconfigure({ url: 'https://github.com/here' })

        expect(isExternalLink(' ')).toBe(false)
    })
    it('returns true for a different site', () => {
        jsdom.reconfigure({ url: 'https://github.com/here' })
        expect(isExternalLink('https://sourcegraph.com/here')).toBe(true)
    })
})

describe('toRepoURL', () => {
    it('generates absolute repo URL without a rev', () => {
        expect(toRepoURL({ repoName: 'sourcegraph/sourcegraph' })).toBe('/sourcegraph/sourcegraph')
    })

    it('generates absolute repo URL with a rev', () => {
        expect(toRepoURL({ repoName: 'sourcegraph/sourcegraph', revision: 'main' })).toBe(
            '/sourcegraph/sourcegraph@main'
        )
    })
})
