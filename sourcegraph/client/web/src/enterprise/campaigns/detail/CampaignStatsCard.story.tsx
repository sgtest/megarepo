import { storiesOf } from '@storybook/react'
import React from 'react'
import { EnterpriseWebStory } from '../../components/EnterpriseWebStory'
import { CampaignStatsCard } from './CampaignStatsCard'

const { add } = storiesOf('web/campaigns/CampaignStatsCard', module).addDecorator(story => (
    <div className="p-3 container web-content">{story()}</div>
))

add('All states', () => (
    <EnterpriseWebStory>
        {props => (
            <CampaignStatsCard
                {...props}
                stats={{
                    closed: 10,
                    deleted: 10,
                    merged: 10,
                    draft: 5,
                    open: 10,
                    total: 100,
                    unpublished: 55,
                }}
                diff={{ added: 1000, changed: 2000, deleted: 1000 }}
                closedAt={null}
            />
        )}
    </EnterpriseWebStory>
))
add('Campaign closed', () => (
    <EnterpriseWebStory>
        {props => (
            <CampaignStatsCard
                {...props}
                stats={{
                    closed: 10,
                    deleted: 10,
                    merged: 10,
                    draft: 0,
                    open: 10,
                    total: 100,
                    unpublished: 60,
                }}
                diff={{ added: 1000, changed: 2000, deleted: 1000 }}
                closedAt={new Date().toISOString()}
            />
        )}
    </EnterpriseWebStory>
))
add('Campaign done', () => (
    <EnterpriseWebStory>
        {props => (
            <CampaignStatsCard
                {...props}
                stats={{
                    deleted: 10,
                    closed: 10,
                    merged: 80,
                    draft: 0,
                    open: 0,
                    total: 100,
                    unpublished: 0,
                }}
                diff={{ added: 1000, changed: 2000, deleted: 1000 }}
                closedAt={null}
            />
        )}
    </EnterpriseWebStory>
))
