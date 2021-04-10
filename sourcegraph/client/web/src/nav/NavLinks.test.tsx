import { render } from 'enzyme'
import * as H from 'history'
import { noop } from 'lodash'
import React from 'react'
import { MemoryRouter } from 'react-router'
import { NEVER } from 'rxjs'

import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { pretendRemote } from '@sourcegraph/shared/src/api/util'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'

import { AuthenticatedUser } from '../auth'
import { KeyboardShortcutsProps } from '../keyboardShortcuts/keyboardShortcuts'
import { ThemePreference } from '../theme'
import { eventLogger } from '../tracking/eventLogger'

import { NavLinks } from './NavLinks'

describe('NavLinks', () => {
    const NOOP_EXTENSIONS_CONTROLLER: ExtensionsControllerProps<
        'executeCommand' | 'extHostAPI'
    >['extensionsController'] = {
        executeCommand: () => Promise.resolve(),
        extHostAPI: Promise.resolve(pretendRemote<FlatExtensionHostAPI>({})),
    }
    const NOOP_PLATFORM_CONTEXT = { forceUpdateTooltip: () => undefined, settings: NEVER, sourcegraphURL: '' }
    const KEYBOARD_SHORTCUTS: KeyboardShortcutsProps['keyboardShortcuts'] = []
    const SETTINGS_CASCADE: SettingsCascadeProps['settingsCascade'] = { final: null, subjects: null }
    const USER: AuthenticatedUser = {
        __typename: 'User',
        id: 'TestUserAlice',
        databaseID: 123,
        url: '/users/alice',
        displayName: 'Alice',
        email: 'alice@acme.com',
        username: 'alice',
        avatarURL: null,
        session: { canSignOut: true },
        settingsURL: '#',
        siteAdmin: true,
        tags: [],
        viewerCanAdminister: true,
        organizations: {
            nodes: [
                {
                    id: '0',
                    name: 'acme',
                    displayName: 'Acme Corp',
                    url: '/organizations/acme',
                    settingsURL: '/organizations/acme/settings',
                },
                {
                    id: '1',
                    name: 'beta',
                    displayName: 'Beta Inc',
                    url: '/organizations/beta',
                    settingsURL: '/organizations/beta/settings',
                },
            ],
        },
    }
    const history = H.createMemoryHistory({ keyLength: 0 })
    const commonProps = {
        extensionsController: NOOP_EXTENSIONS_CONTROLLER,
        platformContext: NOOP_PLATFORM_CONTEXT,
        telemetryService: eventLogger,
        isLightTheme: true,
        themePreference: ThemePreference.Light,
        onThemePreferenceChange: noop,
        keyboardShortcuts: KEYBOARD_SHORTCUTS,
        settingsCascade: SETTINGS_CASCADE,
        history,
        isSourcegraphDotCom: false,
        showBatchChanges: true,
        enableCodeMonitoring: true,
    }

    // The 3 main props that affect the desired contents of NavLinks are whether the user is signed
    // in, whether we're on Sourcegraph.com, and the path. Create snapshots of all permutations.
    for (const authenticatedUser of [null, USER]) {
        for (const showDotComMarketing of [false, true]) {
            for (const path of ['/foo', '/search']) {
                const name = [
                    authenticatedUser ? 'authed' : 'unauthed',
                    showDotComMarketing ? 'Sourcegraph.com' : 'self-hosted',
                    path,
                ].join(' ')
                test(name, () => {
                    expect(
                        render(
                            <MemoryRouter>
                                <NavLinks
                                    {...commonProps}
                                    authenticatedUser={authenticatedUser}
                                    showDotComMarketing={showDotComMarketing}
                                    location={H.createLocation(path, history.location)}
                                    isExtensionAlertAnimating={false}
                                    routes={[]}
                                />
                            </MemoryRouter>
                        )
                    ).toMatchSnapshot()
                })
            }
        }
    }
})
