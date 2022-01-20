import { storiesOf } from '@storybook/react'
import { noop } from 'lodash'
import React from 'react'
import { of } from 'rxjs'

import { AggregateStreamingSearchResults } from '@sourcegraph/shared/src/search/stream'
import { EMPTY_SETTINGS_CASCADE } from '@sourcegraph/shared/src/settings/settings'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    HIGHLIGHTED_FILE_LINES_LONG,
    MULTIPLE_SEARCH_RESULT,
    NOOP_PLATFORM_CONTEXT,
} from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { WebStory } from '../../components/WebStory'

import { SearchNotebookQueryBlock } from './SearchNotebookQueryBlock'

const { add } = storiesOf('web/search/notebook/SearchNotebookQueryBlock', module).addDecorator(story => (
    <div className="p-3 container">{story()}</div>
))

const streamingSearchResult: AggregateStreamingSearchResults = {
    state: 'complete',
    results: [...MULTIPLE_SEARCH_RESULT.results],
    filters: MULTIPLE_SEARCH_RESULT.filters,
    progress: {
        durationMs: 500,
        matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
        skipped: [],
    },
}

const noopBlockCallbacks = {
    onRunBlock: noop,
    onBlockInputChange: noop,
    onSelectBlock: noop,
    onMoveBlockSelection: noop,
    onDeleteBlock: noop,
    onMoveBlock: noop,
    onDuplicateBlock: noop,
}

add('default', () => (
    <WebStory>
        {props => (
            <SearchNotebookQueryBlock
                {...props}
                {...noopBlockCallbacks}
                authenticatedUser={null}
                id="query-block-1"
                input="query"
                type="query"
                output={of(streamingSearchResult)}
                isSelected={false}
                isReadOnly={false}
                isOtherBlockSelected={false}
                isMacPlatform={true}
                isSourcegraphDotCom={true}
                searchContextsEnabled={true}
                sourcegraphSearchLanguageId="sourcegraphSearch"
                telemetryService={NOOP_TELEMETRY_SERVICE}
                fetchHighlightedFileLineRanges={() => of(HIGHLIGHTED_FILE_LINES_LONG)}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
                platformContext={NOOP_PLATFORM_CONTEXT}
            />
        )}
    </WebStory>
))

add('selected', () => (
    <WebStory>
        {props => (
            <SearchNotebookQueryBlock
                {...props}
                {...noopBlockCallbacks}
                id="query-block-1"
                input="query"
                type="query"
                output={of(streamingSearchResult)}
                isSelected={true}
                isOtherBlockSelected={false}
                isReadOnly={false}
                isMacPlatform={true}
                isSourcegraphDotCom={true}
                searchContextsEnabled={true}
                sourcegraphSearchLanguageId="sourcegraphSearch"
                telemetryService={NOOP_TELEMETRY_SERVICE}
                fetchHighlightedFileLineRanges={() => of(HIGHLIGHTED_FILE_LINES_LONG)}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
                authenticatedUser={null}
                platformContext={NOOP_PLATFORM_CONTEXT}
            />
        )}
    </WebStory>
))

add('read-only selected', () => (
    <WebStory>
        {props => (
            <SearchNotebookQueryBlock
                {...props}
                {...noopBlockCallbacks}
                id="query-block-1"
                input="query"
                type="query"
                output={of(streamingSearchResult)}
                isSelected={true}
                isReadOnly={true}
                isOtherBlockSelected={false}
                isMacPlatform={true}
                isSourcegraphDotCom={true}
                searchContextsEnabled={true}
                sourcegraphSearchLanguageId="sourcegraphSearch"
                telemetryService={NOOP_TELEMETRY_SERVICE}
                fetchHighlightedFileLineRanges={() => of(HIGHLIGHTED_FILE_LINES_LONG)}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
                authenticatedUser={null}
                platformContext={NOOP_PLATFORM_CONTEXT}
            />
        )}
    </WebStory>
))
