import { boolean } from '@storybook/addon-knobs'
import { DecoratorFn, Story, Meta } from '@storybook/react'
import { of, Observable } from 'rxjs'

import { WebStory } from '../../../../components/WebStory'
import { BatchSpecApplyPreviewConnectionFields, ChangesetApplyPreviewFields } from '../../../../graphql-operations'
import { MultiSelectContextProvider } from '../../MultiSelectContext'
import { filterPublishableIDs } from '../utils'

import { PreviewList } from './PreviewList'
import { hiddenChangesetApplyPreviewStories, visibleChangesetApplyPreviewNodeStories } from './storyData'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/preview',
    decorators: [decorator],
}

export default config

const queryEmptyFileDiffs = () => of({ totalCount: 0, pageInfo: { endCursor: null, hasNextPage: false }, nodes: [] })

export const PreviewListStory: Story = () => {
    const publicationStateSet = boolean('publication state set by spec file', false)

    const nodes: ChangesetApplyPreviewFields[] = [
        ...Object.values(visibleChangesetApplyPreviewNodeStories(publicationStateSet)),
        ...Object.values(hiddenChangesetApplyPreviewStories),
    ]

    const queryChangesetApplyPreview = (): Observable<BatchSpecApplyPreviewConnectionFields> =>
        of({
            pageInfo: {
                endCursor: null,
                hasNextPage: false,
            },
            totalCount: nodes.length,
            nodes,
        })

    const queryPublishableChangesetSpecIDs = (): Observable<string[]> =>
        of(filterPublishableIDs(Object.values(visibleChangesetApplyPreviewNodeStories(publicationStateSet))))

    return (
        <WebStory>
            {props => (
                <MultiSelectContextProvider>
                    <PreviewList
                        {...props}
                        batchSpecID="123123"
                        authenticatedUser={{
                            url: '/users/alice',
                            displayName: 'Alice',
                            username: 'alice',
                            email: 'alice@email.test',
                        }}
                        queryChangesetApplyPreview={queryChangesetApplyPreview}
                        queryChangesetSpecFileDiffs={queryEmptyFileDiffs}
                        queryPublishableChangesetSpecIDs={queryPublishableChangesetSpecIDs}
                    />
                </MultiSelectContextProvider>
            )}
        </WebStory>
    )
}

PreviewListStory.parameters = {
    chromatic: {
        viewports: [320, 576, 978, 1440],
    },
}

PreviewListStory.storyName = 'PreviewList'
