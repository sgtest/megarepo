import React from 'react'
import { CampaignActionsBar } from './CampaignActionsBar'
import { BackgroundProcessState } from '../../../../../shared/src/graphql/schema'
import { shallow } from 'enzyme'

const PROPS = {
    name: 'Super campaign',
    formID: 'form1',
    onNameChange: () => undefined,
    onEdit: () => undefined,
    // eslint-disable-next-line @typescript-eslint/require-await
    onClose: async () => undefined,
    // eslint-disable-next-line @typescript-eslint/require-await
    onDelete: async () => undefined,
}

describe('CampaignActionsBar', () => {
    test('new with patch set', () =>
        expect(
            shallow(<CampaignActionsBar {...PROPS} mode="viewing" previewingPatchSet={true} campaign={undefined} />)
        ).toMatchSnapshot())
    test('new without patch set', () =>
        expect(
            shallow(<CampaignActionsBar {...PROPS} mode="viewing" previewingPatchSet={false} campaign={undefined} />)
        ).toMatchSnapshot())
    test('not editable', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="viewing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: false,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('editable', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="viewing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('closed', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="viewing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: new Date().toISOString(),
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('edit mode', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="editing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('processing', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="editing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.PROCESSING,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('mode: saving', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="saving"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('mode: deleting', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="deleting"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('mode: closing', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="closing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('some changesets still open', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="viewing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
    test('all changesets not open', () =>
        expect(
            shallow(
                <CampaignActionsBar
                    {...PROPS}
                    mode="viewing"
                    previewingPatchSet={false}
                    campaign={{
                        closedAt: null,
                        name: 'Super campaign',
                        status: {
                            state: BackgroundProcessState.COMPLETED,
                        },
                        viewerCanAdminister: true,
                    }}
                />
            )
        ).toMatchSnapshot())
})
