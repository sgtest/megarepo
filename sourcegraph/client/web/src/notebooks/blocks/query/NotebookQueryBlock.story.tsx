import { DecoratorFn, Story, Meta } from '@storybook/react'
import { noop } from 'lodash'
import { of } from 'rxjs'

import { AggregateStreamingSearchResults } from '@sourcegraph/shared/src/search/stream'
import { EMPTY_SETTINGS_CASCADE } from '@sourcegraph/shared/src/settings/settings'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    extensionsController,
    HIGHLIGHTED_FILE_LINES_LONG,
    MULTIPLE_SEARCH_RESULT,
    NOOP_PLATFORM_CONTEXT,
} from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { WebStory } from '../../../components/WebStory'

import { NotebookQueryBlock } from './NotebookQueryBlock'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/search/notebooks/blocks/query/NotebookQueryBlock',
    decorators: [decorator],
}

export default config

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

export const Default: Story = () => (
    <WebStory>
        {props => (
            <NotebookQueryBlock
                {...props}
                {...noopBlockCallbacks}
                authenticatedUser={null}
                id="query-block-1"
                input={{ query: 'query' }}
                output={of(streamingSearchResult)}
                isSelected={false}
                isReadOnly={false}
                isOtherBlockSelected={false}
                isSourcegraphDotCom={true}
                searchContextsEnabled={true}
                globbing={false}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                fetchHighlightedFileLineRanges={() => of(HIGHLIGHTED_FILE_LINES_LONG)}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
                platformContext={NOOP_PLATFORM_CONTEXT}
                extensionsController={extensionsController}
            />
        )}
    </WebStory>
)

export const Selected: Story = () => (
    <WebStory>
        {props => (
            <NotebookQueryBlock
                {...props}
                {...noopBlockCallbacks}
                id="query-block-1"
                input={{ query: 'query' }}
                output={of(streamingSearchResult)}
                isSelected={true}
                isOtherBlockSelected={false}
                isReadOnly={false}
                isSourcegraphDotCom={true}
                searchContextsEnabled={true}
                globbing={false}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                fetchHighlightedFileLineRanges={() => of(HIGHLIGHTED_FILE_LINES_LONG)}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
                authenticatedUser={null}
                platformContext={NOOP_PLATFORM_CONTEXT}
                extensionsController={extensionsController}
            />
        )}
    </WebStory>
)

export const ReadOnlySelected: Story = () => (
    <WebStory>
        {props => (
            <NotebookQueryBlock
                {...props}
                {...noopBlockCallbacks}
                id="query-block-1"
                input={{ query: 'query' }}
                output={of(streamingSearchResult)}
                isSelected={true}
                isReadOnly={true}
                isOtherBlockSelected={false}
                isSourcegraphDotCom={true}
                searchContextsEnabled={true}
                globbing={false}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                fetchHighlightedFileLineRanges={() => of(HIGHLIGHTED_FILE_LINES_LONG)}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
                authenticatedUser={null}
                platformContext={NOOP_PLATFORM_CONTEXT}
                extensionsController={extensionsController}
            />
        )}
    </WebStory>
)

ReadOnlySelected.storyName = 'read-only selected'
