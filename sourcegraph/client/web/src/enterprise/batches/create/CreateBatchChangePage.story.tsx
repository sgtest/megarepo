import { storiesOf } from '@storybook/react'

import {
    EMPTY_SETTINGS_CASCADE,
    SettingsOrgSubject,
    SettingsUserSubject,
} from '@sourcegraph/shared/src/settings/settings'

import { WebStory } from '../../../components/WebStory'

import { CreateBatchChangePage } from './CreateBatchChangePage'

const { add } = storiesOf('web/batches/CreateBatchChangePage', module)
    .addDecorator(story => <div className="p-3 container">{story()}</div>)
    .addParameters({
        chromatic: {
            disableSnapshot: false,
        },
    })

add('experimental execution disabled', () => (
    <WebStory>
        {props => <CreateBatchChangePage headingElement="h1" {...props} settingsCascade={EMPTY_SETTINGS_CASCADE} />}
    </WebStory>
))

const FIXTURE_ORG: SettingsOrgSubject = {
    __typename: 'Org',
    name: 'sourcegraph',
    displayName: 'Sourcegraph',
    id: 'a',
    viewerCanAdminister: true,
}

const FIXTURE_USER: SettingsUserSubject = {
    __typename: 'User',
    username: 'alice',
    displayName: 'alice',
    id: 'b',
    viewerCanAdminister: true,
}

add('experimental execution enabled', () => (
    <WebStory>
        {props => (
            <div style={{ height: '95vh', width: '100%' }}>
                <CreateBatchChangePage
                    headingElement="h1"
                    {...props}
                    settingsCascade={{
                        ...EMPTY_SETTINGS_CASCADE,
                        final: {
                            experimentalFeatures: { batchChangesExecution: true },
                        },
                        subjects: [
                            { subject: FIXTURE_ORG, settings: { a: 1 }, lastID: 1 },
                            { subject: FIXTURE_USER, settings: { b: 2 }, lastID: 2 },
                        ],
                    }}
                />
            </div>
        )}
    </WebStory>
))
