import * as H from 'history'
import React from 'react'
import { MemoryRouter } from 'react-router'
import renderer from 'react-test-renderer'
import * as GQL from '../../../shared/src/graphql/schema'
import { ThemePreference } from '../theme'
import { UserNavItem } from './UserNavItem'

describe('UserNavItem', () => {
    const ORG_CONNECTION = {
        __typename: 'OrgConnection',
        nodes: [
            { id: '1', displayName: 'd', settingsURL: 'u' },
            { id: '2', name: 'n', settingsURL: 'u' },
        ] as unknown,
        totalCount: 2,
    } as GQL.IOrgConnection
    const USER = { username: 'u', url: '/u', settingsURL: '/u/settings', organizations: ORG_CONNECTION } as GQL.IUser

    const history = H.createMemoryHistory({ keyLength: 0 })

    test('simple', () => {
        expect(
            renderer
                .create(
                    <MemoryRouter>
                        <UserNavItem
                            isLightTheme={true}
                            onThemePreferenceChange={() => undefined}
                            themePreference={ThemePreference.Light}
                            location={history.location}
                            authenticatedUser={USER}
                            showDotComMarketing={true}
                            showDiscussions={true}
                        />
                    </MemoryRouter>
                )
                .toJSON()
        ).toMatchSnapshot()
    })
})
