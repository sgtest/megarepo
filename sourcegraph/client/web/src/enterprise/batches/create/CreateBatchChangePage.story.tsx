import { storiesOf } from '@storybook/react'
import React from 'react'

import { EMPTY_SETTINGS_CASCADE } from '@sourcegraph/shared/src/settings/settings'

import { EnterpriseWebStory } from '../../components/EnterpriseWebStory'

import { CreateBatchChangePage } from './CreateBatchChangePage'

const { add } = storiesOf('web/batches/CreateBatchChangePage', module).addDecorator(story => (
    <div className="p-3 container">{story()}</div>
))

add('Page', () => (
    <EnterpriseWebStory>
        {props => <CreateBatchChangePage headingElement="h1" {...props} settingsCascade={EMPTY_SETTINGS_CASCADE} />}
    </EnterpriseWebStory>
))
