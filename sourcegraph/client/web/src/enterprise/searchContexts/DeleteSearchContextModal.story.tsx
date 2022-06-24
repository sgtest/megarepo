import { DecoratorFn, Meta, Story } from '@storybook/react'
import { NEVER } from 'rxjs'
import sinon from 'sinon'

import { ISearchContext } from '@sourcegraph/shared/src/schema'
import { NOOP_PLATFORM_CONTEXT } from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { WebStory } from '../../components/WebStory'

import { DeleteSearchContextModal } from './DeleteSearchContextModal'

const searchContext = {
    __typename: 'SearchContext',
    id: '1',
} as ISearchContext

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/enterprise/searchContexts/DeleteSearchContextModal',
    decorators: [decorator],
    parameters: {
        chromatic: { viewports: [1200], disableSnapshot: false },
    },
}

export default config

export const DeleteSearchContextModalStory: Story = () => (
    <WebStory>
        {webProps => (
            <DeleteSearchContextModal
                {...webProps}
                isOpen={true}
                searchContext={searchContext}
                toggleDeleteModal={sinon.fake()}
                deleteSearchContext={sinon.fake(() => NEVER)}
                platformContext={NOOP_PLATFORM_CONTEXT}
            />
        )}
    </WebStory>
)

DeleteSearchContextModalStory.storyName = 'DeleteSearchContextModal'
