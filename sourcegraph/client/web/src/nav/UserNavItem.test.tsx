import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import sinon from 'sinon'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'
import { AnchorLink, RouterLink, setLinkComponent } from '@sourcegraph/wildcard'
import { renderWithBrandedContext } from '@sourcegraph/wildcard/src/testing'

import { UserNavItem, UserNavItemProps } from './UserNavItem'

describe('UserNavItem', () => {
    beforeAll(() => {
        setLinkComponent(RouterLink)
    })

    afterAll(() => {
        setLinkComponent(AnchorLink)
    })

    const USER: UserNavItemProps['authenticatedUser'] = {
        username: 'alice',
        displayName: 'alice doe',
        avatarURL: null,
        session: { canSignOut: true },
        settingsURL: '#',
        siteAdmin: true,
        organizations: {
            nodes: [
                {
                    __typename: 'Org',
                    id: '0',
                    name: 'acme',
                    displayName: 'Acme Corp',
                    url: '/organizations/acme',
                    settingsURL: '/organizations/acme/settings',
                },
                {
                    __typename: 'Org',
                    id: '1',
                    name: 'beta',
                    displayName: 'Beta Inc',
                    url: '/organizations/beta',
                    settingsURL: '/organizations/beta/settings',
                },
            ],
        },
    }

    test('simple', () => {
        expect(
            render(
                <MemoryRouter>
                    <MockedTestProvider>
                        <UserNavItem
                            showKeyboardShortcutsHelp={() => undefined}
                            authenticatedUser={USER}
                            isSourcegraphDotCom={true}
                            isSourcegraphApp={false}
                            showFeedbackModal={() => undefined}
                            telemetryService={NOOP_TELEMETRY_SERVICE}
                        />
                    </MockedTestProvider>
                </MemoryRouter>
            ).asFragment()
        ).toMatchSnapshot()
    })

    test('logout click triggers page refresh instead of performing client-side only navigation', async () => {
        const result = renderWithBrandedContext(
            <MockedTestProvider>
                <UserNavItem
                    showKeyboardShortcutsHelp={() => undefined}
                    authenticatedUser={USER}
                    isSourcegraphDotCom={true}
                    isSourcegraphApp={false}
                    showFeedbackModal={() => undefined}
                    telemetryService={NOOP_TELEMETRY_SERVICE}
                />
            </MockedTestProvider>
        )

        // Prevent console.error cause by "Not implemented: navigation (except hash changes)"
        // https://github.com/jsdom/jsdom/issues/2112
        sinon.stub(console, 'error')
        userEvent.click(screen.getByRole('button'))
        userEvent.click(await screen.findByText('Sign out'))

        expect(result.locationRef.entries.length).toBe(1)
        expect(result.locationRef.entries.find(({ pathname }) => pathname.includes('sign-out'))).toBe(undefined)
    })
})
