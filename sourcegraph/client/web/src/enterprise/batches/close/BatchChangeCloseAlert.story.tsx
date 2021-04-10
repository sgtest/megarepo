import { boolean } from '@storybook/addon-knobs'
import { useState } from '@storybook/addons'
import { storiesOf } from '@storybook/react'
import React from 'react'

import { EnterpriseWebStory } from '../../components/EnterpriseWebStory'

import { BatchChangeCloseAlert } from './BatchChangeCloseAlert'

const { add } = storiesOf('web/batches/close/BatchChangeCloseAlert', module)
    .addDecorator(story => <div className="p-3 container web-content">{story()}</div>)
    .addParameters({
        chromatic: {
            viewports: [320, 576, 978, 1440],
        },
    })

add('Has open changesets', () => {
    const [closeChangesets, setCloseChangesets] = useState(false)
    return (
        <EnterpriseWebStory>
            {props => (
                <BatchChangeCloseAlert
                    {...props}
                    batchChangeID="change123"
                    batchChangeURL="/users/john/batch-changes/change123"
                    totalCount={10}
                    closeChangesets={closeChangesets}
                    setCloseChangesets={setCloseChangesets}
                    viewerCanAdminister={boolean('viewerCanAdminister', true)}
                    closeBatchChange={() => Promise.resolve()}
                />
            )}
        </EnterpriseWebStory>
    )
})
add('No open changesets', () => {
    const [closeChangesets, setCloseChangesets] = useState(false)
    return (
        <EnterpriseWebStory>
            {props => (
                <BatchChangeCloseAlert
                    {...props}
                    batchChangeID="change123"
                    batchChangeURL="/users/john/batch-changes/change123"
                    totalCount={0}
                    closeChangesets={closeChangesets}
                    setCloseChangesets={setCloseChangesets}
                    viewerCanAdminister={boolean('viewerCanAdminister', true)}
                    closeBatchChange={() => Promise.resolve()}
                />
            )}
        </EnterpriseWebStory>
    )
})
