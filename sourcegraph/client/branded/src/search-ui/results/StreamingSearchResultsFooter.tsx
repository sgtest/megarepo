import React from 'react'

import classNames from 'classnames'

import { AggregateStreamingSearchResults } from '@sourcegraph/shared/src/search/stream'
import { Alert, LoadingSpinner, Code, Text, H2, H3, ErrorAlert } from '@sourcegraph/wildcard'

import { StreamingProgressCount } from './progress/StreamingProgressCount'

import styles from './StreamingSearchResultsList.module.scss'

export const StreamingSearchResultFooter: React.FunctionComponent<
    React.PropsWithChildren<{
        results?: AggregateStreamingSearchResults
        children?: React.ReactChild | React.ReactChild[]
    }>
> = ({ results, children }) => (
    <div className={classNames(styles.contentCentered, 'd-flex flex-column align-items-center')}>
        {(!results || results?.state === 'loading') && (
            <div className="text-center my-4" data-testid="loading-container">
                <LoadingSpinner />
            </div>
        )}

        {results?.state === 'complete' && results?.results.length > 0 && (
            <StreamingProgressCount progress={results.progress} state={results.state} className="mt-4 mb-2" />
        )}

        {results?.state === 'error' && (
            <ErrorAlert className="m-3" data-testid="search-results-list-error" error={results.error} />
        )}

        {results?.state === 'complete' && !results.alert && results?.results.length === 0 && (
            <div className="pr-3 mt-3 align-self-stretch">
                <Alert variant="info">
                    <H3 as={H2} className="m-0 py-1">
                        No results matched your search.
                    </H3>
                </Alert>
            </div>
        )}

        {results?.state === 'complete' &&
            results.progress.skipped.some(skipped => skipped.reason.includes('-limit')) && (
                <Alert className="d-flex m-3" variant="info">
                    <Text className="m-0">
                        <strong>Result limit hit.</strong> Modify your search with <Code>count:</Code> to return
                        additional items.
                    </Text>
                </Alert>
            )}

        {children}
    </div>
)
