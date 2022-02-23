import { useCallback } from '@storybook/addons'
import { storiesOf } from '@storybook/react'
import React from 'react'
import { of } from 'rxjs'

import { EMPTY_SETTINGS_CASCADE } from '@sourcegraph/shared/src/settings/settings'

import { WebStory } from '../../../components/WebStory'

import { BatchChangeListPage } from './BatchChangeListPage'
import { nodes } from './testData'

const { add } = storiesOf('web/batches/list/BatchChangeListPage', module)
    .addDecorator(story => <div className="p-3 container">{story()}</div>)
    .addParameters({
        chromatic: {
            viewports: [320, 576, 978, 1440],
            disableSnapshot: false,
        },
    })

const queryBatchChanges = () =>
    of({
        batchChanges: {
            totalCount: Object.values(nodes).length,
            nodes: Object.values(nodes),
            pageInfo: { endCursor: null, hasNextPage: false },
        },
        totalCount: Object.values(nodes).length,
    })

const batchChangesNotLicensed = () => of(false)

const batchChangesLicensed = () => of(true)

add('List of batch changes', () => (
    <WebStory>
        {props => (
            <BatchChangeListPage
                {...props}
                headingElement="h1"
                canCreate={true}
                queryBatchChanges={queryBatchChanges}
                areBatchChangesLicensed={batchChangesLicensed}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
            />
        )}
    </WebStory>
))

add('List of batch changes, server-side execution enabled', () => (
    <WebStory>
        {props => (
            <BatchChangeListPage
                {...props}
                headingElement="h1"
                canCreate={true}
                queryBatchChanges={queryBatchChanges}
                areBatchChangesLicensed={batchChangesLicensed}
                settingsCascade={{
                    ...EMPTY_SETTINGS_CASCADE,
                    final: {
                        experimentalFeatures: { batchChangesExecution: true },
                    },
                }}
            />
        )}
    </WebStory>
))

add('Licensing not enforced', () => (
    <WebStory>
        {props => (
            <BatchChangeListPage
                {...props}
                headingElement="h1"
                canCreate={true}
                queryBatchChanges={queryBatchChanges}
                areBatchChangesLicensed={batchChangesNotLicensed}
                settingsCascade={EMPTY_SETTINGS_CASCADE}
            />
        )}
    </WebStory>
))

add('No batch changes', () => {
    const queryBatchChanges = useCallback(
        () =>
            of({
                batchChanges: {
                    totalCount: 0,
                    nodes: [],
                    pageInfo: {
                        endCursor: null,
                        hasNextPage: false,
                    },
                },
                totalCount: 0,
            }),
        []
    )
    return (
        <WebStory>
            {props => (
                <BatchChangeListPage
                    {...props}
                    headingElement="h1"
                    canCreate={true}
                    queryBatchChanges={queryBatchChanges}
                    areBatchChangesLicensed={batchChangesLicensed}
                    settingsCascade={EMPTY_SETTINGS_CASCADE}
                />
            )}
        </WebStory>
    )
})

const QUERY_NO_BATCH_CHANGES = () =>
    of({
        batchChanges: {
            totalCount: 0,
            nodes: [],
            pageInfo: {
                endCursor: null,
                hasNextPage: false,
            },
        },
        totalCount: 0,
    })

add('All batch changes tab empty', () => (
    <WebStory>
        {props => (
            <BatchChangeListPage
                {...props}
                headingElement="h1"
                canCreate={true}
                queryBatchChanges={QUERY_NO_BATCH_CHANGES}
                areBatchChangesLicensed={batchChangesLicensed}
                openTab="batchChanges"
                settingsCascade={EMPTY_SETTINGS_CASCADE}
            />
        )}
    </WebStory>
))

add('All batch changes tab empty, cannot create', () => (
    <WebStory>
        {props => (
            <BatchChangeListPage
                {...props}
                headingElement="h1"
                canCreate={false}
                queryBatchChanges={QUERY_NO_BATCH_CHANGES}
                areBatchChangesLicensed={batchChangesLicensed}
                openTab="batchChanges"
                settingsCascade={EMPTY_SETTINGS_CASCADE}
            />
        )}
    </WebStory>
))
