import classNames from 'classnames'
import { snakeCase } from 'lodash'
import React, { useEffect } from 'react'

import { isHTTPAuthError } from '@sourcegraph/shared/src/backend/fetch'
import { ErrorLike, isErrorLike } from '@sourcegraph/shared/src/util/errors'

import { SourcegraphIconButton, SourcegraphIconButtonProps } from '../../components/SourcegraphIconButton'
import { DEFAULT_SOURCEGRAPH_URL, getPlatformName } from '../../util/context'

import { CodeHostContext } from './codeHost'
import { SignInButton } from './SignInButton'

export interface ViewOnSourcegraphButtonClassProps {
    className?: string
    iconClassName?: string
}

interface ViewOnSourcegraphButtonProps extends ViewOnSourcegraphButtonClassProps {
    codeHostType: string
    getContext: () => CodeHostContext
    sourcegraphURL: string
    minimalUI: boolean
    repoExistsOrError?: boolean | ErrorLike
    showSignInButton?: boolean
    onConfigureSourcegraphClick?: React.MouseEventHandler<HTMLAnchorElement>

    /**
     * A callback for when the user finished a sign in flow.
     * This does not guarantee the sign in was successful.
     */
    onSignInClose?: () => void

    onPrivateCloudError: (hasError: boolean) => void
}

export const ViewOnSourcegraphButton: React.FunctionComponent<ViewOnSourcegraphButtonProps> = ({
    codeHostType,
    repoExistsOrError,
    sourcegraphURL,
    getContext,
    minimalUI,
    onConfigureSourcegraphClick,
    showSignInButton,
    onSignInClose,
    className,
    iconClassName,
    onPrivateCloudError,
}) => {
    className = classNames('open-on-sourcegraph', className)
    const mutedIconClassName = classNames('open-on-sourcegraph__icon--muted', iconClassName)
    const commonProps: Partial<SourcegraphIconButtonProps> = {
        className,
        iconClassName,
    }

    const { rawRepoName, revision, privateRepository } = getContext()

    const isPrivateCloudError =
        sourcegraphURL === DEFAULT_SOURCEGRAPH_URL && repoExistsOrError === false && privateRepository

    useEffect(() => {
        onPrivateCloudError(isPrivateCloudError)

        return () => {
            onPrivateCloudError(false)
        }
    }, [isPrivateCloudError, onPrivateCloudError])

    // Show nothing while loading
    if (repoExistsOrError === undefined) {
        return null
    }

    const url = new URL(
        `/${rawRepoName}${revision ? `@${revision}` : ''}?utm_source=${getPlatformName()}`,
        sourcegraphURL
    ).href

    if (isErrorLike(repoExistsOrError)) {
        // If the problem is the user is not signed in, show a sign in CTA (if not shown elsewhere)
        if (isHTTPAuthError(repoExistsOrError)) {
            if (showSignInButton) {
                return <SignInButton {...commonProps} sourcegraphURL={sourcegraphURL} onSignInClose={onSignInClose} />
            }
            // Sign in button may already be shown elsewhere on the page
            return null
        }

        const commonErrorCaseProps: Partial<SourcegraphIconButtonProps> = {
            ...commonProps,
            // If we are not running in the browser extension where we can open the options menu,
            // open the documentation for how to configure the code host we are on.
            href: new URL(snakeCase(codeHostType), 'https://docs.sourcegraph.com/integration/').href,
            // onClick can call preventDefault() to prevent that and take a different action (opening the options menu).
            onClick: onConfigureSourcegraphClick,
        }

        // If there was an unexpected error, show it in the tooltip.
        // Still link to the Sourcegraph instance in native integrations
        // as that might explain the error (e.g. not reachable).
        // In the browser extension, let the onConfigureSourcegraphClick handler can handle this.
        return (
            <SourcegraphIconButton
                {...commonErrorCaseProps}
                iconClassName={mutedIconClassName}
                href={url}
                label="Error"
                title={repoExistsOrError.message}
                ariaLabel={repoExistsOrError.message}
            />
        )
    }

    if (isPrivateCloudError) {
        return (
            <SourcegraphIconButton
                {...commonProps}
                href={new URL(snakeCase(codeHostType), 'https://docs.sourcegraph.com/integration/').href}
                onClick={onConfigureSourcegraphClick}
                label="Configure Sourcegraph"
                title="Set up Sourcegraph for search and code intelligence on private repositories"
                ariaLabel="Set up Sourcegraph for search and code intelligence on private repositories"
            />
        )
    }

    // If the repository does not exist, communicate that to explain why e.g. code intelligence does not work
    if (!repoExistsOrError) {
        return (
            <SourcegraphIconButton
                {...commonProps}
                href={url} // Still link to the repository (which will show a not found page, and can link to further actions)
                iconClassName={mutedIconClassName}
                label="Repository not found"
                title={`The repository does not exist on the configured Sourcegraph instance ${sourcegraphURL}`}
                ariaLabel={`The repository does not exist on the configured Sourcegraph instance ${sourcegraphURL}`}
            />
        )
    }

    // Otherwise don't render anything in minimal UI mode
    if (minimalUI) {
        return null
    }

    // Render a "View on Sourcegraph" button
    return (
        <SourcegraphIconButton
            {...commonProps}
            href={url}
            title="View repository on Sourcegraph"
            ariaLabel="View repository on Sourcegraph"
        />
    )
}
