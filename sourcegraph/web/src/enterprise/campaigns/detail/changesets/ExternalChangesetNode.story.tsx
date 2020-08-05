import * as H from 'history'
import { storiesOf } from '@storybook/react'
import { radios, boolean } from '@storybook/addon-knobs'
import React from 'react'
import webStyles from '../../../../enterprise.scss'
import { Tooltip } from '../../../../components/tooltip/Tooltip'
import { ExternalChangesetNode } from './ExternalChangesetNode'
import { addHours } from 'date-fns'
import {
    ChangesetExternalState,
    ChangesetCheckState,
    ChangesetReviewState,
    ChangesetReconcilerState,
    ChangesetPublicationState,
} from '../../../../../../shared/src/graphql/schema'

let isLightTheme = true

const { add } = storiesOf('web/campaigns/ExternalChangesetNode', module).addDecorator(story => {
    const theme = radios('Theme', { Light: 'light', Dark: 'dark' }, 'light')
    document.body.classList.toggle('theme-light', theme === 'light')
    document.body.classList.toggle('theme-dark', theme === 'dark')
    isLightTheme = theme === 'light'
    return (
        <>
            <Tooltip />
            <style>{webStyles}</style>
            <div className="p-3 container">{story()}</div>
        </>
    )
})

add('All external states', () => {
    const now = new Date()
    const history = H.createMemoryHistory()
    return (
        <>
            {Object.values(ChangesetExternalState).map((externalState, index) => (
                <ExternalChangesetNode
                    key={index}
                    node={{
                        id: 'somechangeset',
                        updatedAt: now.toISOString(),
                        nextSyncAt: addHours(now, 1).toISOString(),
                        externalState,
                        __typename: 'ExternalChangeset',
                        title: 'Changeset title on code host',
                        reconcilerState: ChangesetReconcilerState.COMPLETED,
                        publicationState: ChangesetPublicationState.PUBLISHED,
                        body: 'This changeset does the following things:\nIs awesome\nIs useful',
                        checkState: ChangesetCheckState.PENDING,
                        createdAt: now.toISOString(),
                        externalID: '123',
                        externalURL: {
                            url: 'http://test.test/pr/123',
                        },
                        diffStat: {
                            added: 10,
                            changed: 20,
                            deleted: 8,
                        },
                        labels: [],
                        repository: {
                            id: 'repoid',
                            name: 'github.com/sourcegraph/sourcegraph',
                            url: 'http://test.test/sourcegraph/sourcegraph',
                        },
                        reviewState: ChangesetReviewState.COMMENTED,
                    }}
                    history={history}
                    location={history.location}
                    isLightTheme={isLightTheme}
                    viewerCanAdminister={boolean('viewerCanAdminister', true)}
                />
            ))}
        </>
    )
})
