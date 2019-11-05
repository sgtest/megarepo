import * as H from 'history'
import { flatten, noop } from 'lodash'
import React from 'react'
import { createRenderer } from 'react-test-renderer/shallow'
import { setLinkComponent } from '../../../shared/src/components/Link'
import { ExtensionsControllerProps } from '../../../shared/src/extensions/controller'
import * as GQL from '../../../shared/src/graphql/schema'
import { SettingsCascadeProps } from '../../../shared/src/settings/settings'
import { KeyboardShortcutsProps } from '../../../shared/src/keyboardShortcuts'
import { eventLogger } from '../tracking/eventLogger'
import { NavLinks } from './NavLinks'
import { ThemePreference } from '../search/theme'

// Renders a human-readable list of the NavLinks' contents so that humans can more easily diff
// snapshots to see what actually changed.
const renderShallow = (element: React.ReactElement<NavLinks['props']>): any => {
    const renderer = createRenderer()
    renderer.render(element)

    const getDisplayName = (element: React.ReactChild): string | string[] => {
        if (element === null) {
            return []
        }
        if (typeof element === 'string' || typeof element === 'number') {
            return element.toString()
        }
        if (element.type === 'li' && (element.props.children.props.href || element.props.children.props.to)) {
            return `${element.props.children.props.children} ${element.props.children.props.href ||
                element.props.children.props.to}`
        }
        if (typeof element.type === 'symbol' || typeof element.type === 'string') {
            return flatten(React.Children.map(element.props.children, element => getDisplayName(element)))
        }
        return (element.type as any).displayName || element.type.name || 'Unknown'
    }

    return flatten(
        React.Children.map(renderer.getRenderOutput().props.children, e => getDisplayName(e)).filter(e => !!e)
    )
}

describe('NavLinks', () => {
    setLinkComponent((props: any) => <a {...props} />)
    afterAll(() => setLinkComponent(null as any)) // reset global env for other tests
    const NOOP_EXTENSIONS_CONTROLLER: ExtensionsControllerProps<
        'executeCommand' | 'services'
    >['extensionsController'] = { executeCommand: () => Promise.resolve(), services: {} as any }
    const NOOP_PLATFORM_CONTEXT = { forceUpdateTooltip: () => undefined }
    const KEYBOARD_SHORTCUTS: KeyboardShortcutsProps['keyboardShortcuts'] = []
    const SETTINGS_CASCADE: SettingsCascadeProps['settingsCascade'] = { final: null, subjects: null }
    // eslint-disable-next-line @typescript-eslint/no-object-literal-type-assertion
    const USER = { username: 'u' } as GQL.IUser
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
        showCampaigns: true,
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
                        renderShallow(
                            <NavLinks
                                {...commonProps}
                                authenticatedUser={authenticatedUser}
                                showDotComMarketing={showDotComMarketing}
                                location={H.createLocation(path, history.location)}
                            />
                        )
                    ).toMatchSnapshot()
                })
            }
        }
    }
})
