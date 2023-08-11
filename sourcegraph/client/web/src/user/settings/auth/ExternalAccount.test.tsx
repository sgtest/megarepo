import { render } from '@testing-library/react'
import GithubIcon from 'mdi-react/GithubIcon'

import type { AuthProvider } from '../../../jscontext'

import { ExternalAccountConnectionDetails } from './ExternalAccount'
import type { NormalizedExternalAccount } from './ExternalAccountsSignIn'

const mockAccount: NormalizedExternalAccount = {
    name: 'Github',
    icon: ({ className }) => <GithubIcon className={className} />,
}

describe('ExternalAccountConnectionDetails', () => {
    const serviceTypes: AuthProvider['serviceType'][] = [
        'github',
        'gitlab',
        'bitbucketCloud',
        'http-header',
        'openidconnect',
        'sourcegraph-operator',
        'saml',
        'builtin',
        'gerrit',
        'azuredevops',
    ]

    test("renders correctly when display name isn't set", () => {
        for (const serviceType of serviceTypes) {
            const cmp = render(<ExternalAccountConnectionDetails account={mockAccount} serviceType={serviceType} />)
            expect(cmp.asFragment()).toMatchSnapshot()
        }
    })

    test('renders correctly when display name is set', () => {
        for (const serviceType of serviceTypes) {
            const cmp = render(
                <ExternalAccountConnectionDetails
                    account={{
                        ...mockAccount,
                        external: {
                            id: '123',
                            displayName: 'test@sourcegraph.com',
                            login: 'test',
                            url: 'https://example.com',
                        },
                    }}
                    serviceType={serviceType}
                />
            )
            expect(cmp.asFragment()).toMatchSnapshot()
        }
    })
})
