import { getMonacoTokens } from './tokens'
import { scanSearchQuery, ScanSuccess, Sequence } from './scanner'

describe('getMonacoTokens()', () => {
    test('returns the tokens for a parsed search query', () => {
        expect(
            getMonacoTokens(
                (scanSearchQuery('r:^github.com/sourcegraph f:code_intelligence trackViews') as ScanSuccess<Sequence>)
                    .token
            )
        ).toStrictEqual([
            {
                scopes: 'filterKeyword',
                startIndex: 0,
            },
            {
                scopes: 'identifier',
                startIndex: 2,
            },
            {
                scopes: 'whitespace',
                startIndex: 25,
            },
            {
                scopes: 'filterKeyword',
                startIndex: 26,
            },
            {
                scopes: 'identifier',
                startIndex: 28,
            },
            {
                scopes: 'whitespace',
                startIndex: 45,
            },
            {
                scopes: 'identifier',
                startIndex: 46,
            },
        ])
    })

    test('search query containing parenthesized parameters', () => {
        expect(getMonacoTokens((scanSearchQuery('r:a (f:b and c)') as ScanSuccess<Sequence>).token)).toStrictEqual([
            {
                scopes: 'filterKeyword',
                startIndex: 0,
            },
            {
                scopes: 'identifier',
                startIndex: 2,
            },
            {
                scopes: 'whitespace',
                startIndex: 3,
            },
            {
                scopes: 'identifier',
                startIndex: 4,
            },
            {
                scopes: 'filterKeyword',
                startIndex: 5,
            },
            {
                scopes: 'identifier',
                startIndex: 7,
            },
            {
                scopes: 'whitespace',
                startIndex: 8,
            },
            {
                scopes: 'keyword',
                startIndex: 9,
            },
            {
                scopes: 'whitespace',
                startIndex: 12,
            },
            {
                scopes: 'identifier',
                startIndex: 13,
            },
            {
                scopes: 'identifier',
                startIndex: 14,
            },
        ])
    })
})
