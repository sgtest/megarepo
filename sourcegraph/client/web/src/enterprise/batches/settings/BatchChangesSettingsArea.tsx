import React from 'react'

import { PageHeader, Text } from '@sourcegraph/wildcard'

import { PageTitle } from '../../../components/PageTitle'
import { UserAreaUserFields } from '../../../graphql-operations'

import { UserCodeHostConnections } from './CodeHostConnections'
import { RolloutWindowsConfiguration } from './RolloutWindowsConfiguration'

export interface BatchChangesSettingsAreaProps {
    user: Pick<UserAreaUserFields, 'id'>
}

/** The page area for all batch changes settings. It's shown in the user settings sidebar. */
export const BatchChangesSettingsArea: React.FunctionComponent<
    React.PropsWithChildren<BatchChangesSettingsAreaProps>
> = props => (
    <div className="test-batches-settings-page">
        <PageTitle title="Batch changes settings" />
        <PageHeader headingElement="h2" path={[{ text: 'Batch Changes settings' }]} className="mb-3" />
        <RolloutWindowsConfiguration />
        <UserCodeHostConnections
            headerLine={<Text>Add access tokens to enable Batch Changes changeset creation on your code hosts.</Text>}
            userID={props.user.id}
        />
    </div>
)
