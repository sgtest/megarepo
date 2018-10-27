import { withAuthenticatedUser } from '@sourcegraph/webapp/dist/auth/withAuthenticatedUser'
import { PageTitle } from '@sourcegraph/webapp/dist/components/PageTitle'
import * as React from 'react'

/** A page for publishing a new release of an extension to the extension registry. */
export const RegistryExtensionNewReleasePage = withAuthenticatedUser(() => (
    <div className="registry-extension-new-release-page">
        <PageTitle title="Publish new release" />
        <h2>Publish new release</h2>
        <p>
            Use the{' '}
            <a href="https://github.com/sourcegraph/src-cli" target="_blank">
                <code>src</code> CLI tool
            </a>{' '}
            to publish a new release:
        </p>
        <pre>
            <code>$ src extensions publish</code>
        </pre>
    </div>
))
