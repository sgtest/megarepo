import React from 'react'
import * as GQL from '../../../../../shared/src/graphql/schema'
import { CampaignStatus } from './CampaignStatus'
import { createRenderer } from 'react-test-renderer/shallow'
import { createMemoryHistory } from 'history'

const PROPS = {
    afterRetry: () => undefined,
    history: createMemoryHistory(),
}

const CAMPAIGN: Pick<GQL.ICampaign, 'id' | 'closedAt' | 'viewerCanAdminister' | 'publishedAt'> & {
    changesets: Pick<GQL.ICampaign['changesets'], 'totalCount'>
} = {
    id: 'Q2FtcGFpZ246MQ==',
    closedAt: null,
    viewerCanAdminister: true,
    publishedAt: '2020-01-01',
    changesets: {
        totalCount: 0,
    },
}

describe('CampaignStatus', () => {
    for (const viewerCanAdminister of [true, false]) {
        const campaign = { ...CAMPAIGN, viewerCanAdminister }
        describe(`viewerCanAdminister: ${String(viewerCanAdminister)}`, () => {
            test('closed campaign', () =>
                expect(
                    createRenderer().render(
                        <CampaignStatus
                            {...PROPS}
                            campaign={{
                                ...campaign,
                                closedAt: '2020-01-01',
                                viewerCanAdminister,
                                status: {
                                    completedCount: 1,
                                    pendingCount: 0,
                                    errors: [],
                                    state: GQL.BackgroundProcessState.COMPLETED,
                                },
                            }}
                            onPublish={() => undefined}
                        />
                    )
                ).toMatchSnapshot())

            test('drafted campaign', () =>
                expect(
                    createRenderer().render(
                        <CampaignStatus
                            {...PROPS}
                            campaign={{
                                ...campaign,
                                publishedAt: null,
                                status: {
                                    completedCount: 1,
                                    pendingCount: 0,
                                    errors: [],
                                    state: GQL.BackgroundProcessState.COMPLETED,
                                },
                            }}
                            onPublish={() => undefined}
                        />
                    )
                ).toMatchSnapshot())

            test('drafted campaign, some published', () =>
                expect(
                    createRenderer().render(
                        <CampaignStatus
                            {...PROPS}
                            campaign={{
                                ...campaign,
                                publishedAt: null,
                                changesets: { totalCount: 1 },
                                status: {
                                    completedCount: 1,
                                    pendingCount: 0,
                                    errors: [],
                                    state: GQL.BackgroundProcessState.COMPLETED,
                                },
                            }}
                            onPublish={() => undefined}
                        />
                    )
                ).toMatchSnapshot())

            test('campaign processing', () =>
                expect(
                    createRenderer().render(
                        <CampaignStatus
                            {...PROPS}
                            campaign={{
                                ...campaign,
                                status: {
                                    completedCount: 3,
                                    pendingCount: 3,
                                    errors: ['a', 'b'],
                                    state: GQL.BackgroundProcessState.PROCESSING,
                                },
                            }}
                            onPublish={() => undefined}
                        />
                    )
                ).toMatchSnapshot())

            test('campaign errored', () =>
                expect(
                    createRenderer().render(
                        <CampaignStatus
                            {...PROPS}
                            campaign={{
                                ...campaign,
                                status: {
                                    completedCount: 3,
                                    pendingCount: 0,
                                    errors: ['a', 'b'],
                                    state: GQL.BackgroundProcessState.ERRORED,
                                },
                            }}
                            onPublish={() => undefined}
                        />
                    )
                ).toMatchSnapshot())
        })
    }
})
