import { storiesOf } from '@storybook/react'
import { addHours } from 'date-fns'
import React from 'react'

import { ChangesetState } from '../../../../graphql-operations'
import { EnterpriseWebStory } from '../../../components/EnterpriseWebStory'

import { HiddenExternalChangesetNode } from './HiddenExternalChangesetNode'

const { add } = storiesOf('web/batches/HiddenExternalChangesetNode', module).addDecorator(story => (
    <div className="p-3 container web-content batch-change-changesets__grid">{story()}</div>
))

add('All states', () => {
    const now = new Date()
    return (
        <EnterpriseWebStory>
            {props => (
                <>
                    {Object.values(ChangesetState).map((state, index) => (
                        <HiddenExternalChangesetNode
                            key={index}
                            {...props}
                            node={{
                                __typename: 'HiddenExternalChangeset',
                                id: 'somechangeset',
                                updatedAt: now.toISOString(),
                                nextSyncAt: addHours(now, 1).toISOString(),
                                state,
                            }}
                        />
                    ))}
                </>
            )}
        </EnterpriseWebStory>
    )
})
