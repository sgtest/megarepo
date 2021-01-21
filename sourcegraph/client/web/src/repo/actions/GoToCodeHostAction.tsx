import { Position, Range } from '@sourcegraph/extension-api-types'
import { upperFirst, toLower } from 'lodash'
import BitbucketIcon from 'mdi-react/BitbucketIcon'
import ExportIcon from 'mdi-react/ExportIcon'
import GithubIcon from 'mdi-react/GithubIcon'
import React, { useCallback, useMemo, useState } from 'react'
import { merge, of } from 'rxjs'
import { catchError } from 'rxjs/operators'
import { PhabricatorIcon } from '../../../../shared/src/components/icons' // TODO: Switch mdi icon
import { asError, ErrorLike, isErrorLike } from '../../../../shared/src/util/errors'
import { fetchFileExternalLinks } from '../backend'
import { RevisionSpec, FileSpec } from '../../../../shared/src/util/url'
import { ExternalLinkFields, RepositoryFields, ExternalServiceKind } from '../../graphql-operations'
import { useObservable } from '../../../../shared/src/util/useObservable'
import GitlabIcon from 'mdi-react/GitlabIcon'
import { eventLogger } from '../../tracking/eventLogger'
import { InstallBrowserExtensionPopover } from './InstallBrowserExtensionPopover'
import { useLocalStorage } from '../../util/useLocalStorage'

interface GoToCodeHostPopoverProps {
    /**
     * Whether the GoToCodeHostAction can show a popover to install the browser extension.
     * It may still not do so if the popover was permanently dismissed.
     */
    canShowPopover: boolean

    /**
     * Called when the popover is dismissed in any way ("No thanks", "Remind me later" or "Install").
     */
    onPopoverDismissed: () => void
}

interface Props extends RevisionSpec, Partial<FileSpec>, GoToCodeHostPopoverProps {
    repo?: Pick<RepositoryFields, 'name' | 'defaultBranch' | 'externalURLs'> | null
    filePath?: string
    commitRange?: string
    position?: Position
    range?: Range

    externalLinks?: ExternalLinkFields[]

    fetchFileExternalLinks: typeof fetchFileExternalLinks
}

const HAS_PERMANENTLY_DISMISSED_POPUP_KEY = 'has-dismissed-browser-ext-popup'

/**
 * A repository header action that goes to the corresponding URL on an external code host.
 */
export const GoToCodeHostAction: React.FunctionComponent<Props> = props => {
    const [showPopover, setShowPopover] = useState(false)

    const { onPopoverDismissed, repo, revision, filePath } = props

    const [hasPermanentlyDismissedPopup, setHasPermanentlyDismissedPopup] = useLocalStorage(
        HAS_PERMANENTLY_DISMISSED_POPUP_KEY,
        false
    )

    const hijackLink = !hasPermanentlyDismissedPopup && props.canShowPopover

    /**
     * The external links for the current file/dir, or undefined while loading, null while not
     * needed (because not viewing a file/dir), or an error.
     */
    const fileExternalLinksOrError = useObservable<ExternalLinkFields[] | null | undefined | ErrorLike>(
        useMemo(() => {
            if (!repo || !filePath) {
                return of(null)
            }
            return merge(
                of(undefined),
                fetchFileExternalLinks({ repoName: repo.name, revision, filePath }).pipe(
                    catchError(error => [asError(error)])
                )
            )
        }, [repo, revision, filePath])
    )

    /** This is a hard rejection. Never ask the user again. */
    const onRejection = useCallback(() => {
        setHasPermanentlyDismissedPopup(true)
        setShowPopover(false)
        onPopoverDismissed()

        eventLogger.log('BrowserExtensionPopupRejected')
    }, [onPopoverDismissed, setHasPermanentlyDismissedPopup])

    /** This is a soft rejection. Called when user clicks 'Remind me later', ESC, or outside of the modal body */
    const onClose = useCallback(() => {
        onPopoverDismissed()
        setShowPopover(false)

        eventLogger.log('BrowserExtensionPopupClosed')
    }, [onPopoverDismissed])

    /** The user is likely to install the browser extension at this point, so don't show it again. */
    const onClickInstall = useCallback(() => {
        setHasPermanentlyDismissedPopup(true)
        setShowPopover(false)
        onPopoverDismissed()

        eventLogger.log('BrowserExtensionPopupClickedInstall')
    }, [onPopoverDismissed, setHasPermanentlyDismissedPopup])

    const toggle = useCallback(() => {
        if (showPopover) {
            setShowPopover(false)
            return
        }

        if (hijackLink) {
            setShowPopover(true)
        }
    }, [hijackLink, showPopover])

    const onClick = useCallback(
        (event: React.MouseEvent<HTMLAnchorElement, MouseEvent>) => {
            eventLogger.log('GoToCodeHostClicked')

            if (showPopover) {
                event.preventDefault()
                setShowPopover(false)
                return
            }

            if (hijackLink) {
                event.preventDefault()
                setShowPopover(true)
            }
        },
        [hijackLink, showPopover]
    )

    // If the default branch is undefined, set to HEAD
    const defaultBranch =
        (!isErrorLike(props.repo) && props.repo && props.repo.defaultBranch && props.repo.defaultBranch.displayName) ||
        'HEAD'

    // If neither repo or file can be loaded, return null, which will hide all code host icons
    if (!props.repo || isErrorLike(fileExternalLinksOrError)) {
        return null
    }

    let externalURLs: ExternalLinkFields[]
    if (props.externalLinks && props.externalLinks.length > 0) {
        externalURLs = props.externalLinks
    } else if (
        fileExternalLinksOrError === null ||
        fileExternalLinksOrError === undefined ||
        isErrorLike(fileExternalLinksOrError) ||
        fileExternalLinksOrError.length === 0
    ) {
        // If the external link for the more specific resource within the repository is loading or errored, use the
        // repository external link.
        externalURLs = props.repo.externalURLs
    } else {
        externalURLs = fileExternalLinksOrError
    }
    if (externalURLs.length === 0) {
        return null
    }

    // Only show the first external link for now.
    const externalURL = externalURLs[0]

    const { displayName, icon } = serviceKindDisplayNameAndIcon(externalURL.serviceKind)
    const Icon = icon || ExportIcon

    // Extract url to add branch, line numbers or commit range.
    let url = externalURL.url
    if (
        externalURL.serviceKind === ExternalServiceKind.GITHUB ||
        externalURL.serviceKind === ExternalServiceKind.GITLAB
    ) {
        // If in a branch, add branch path to the code host URL.
        if (props.revision && props.revision !== defaultBranch && !fileExternalLinksOrError) {
            url += `/tree/${props.revision}`
        }
        // If showing a comparison, add comparison specifier to the code host URL.
        if (props.commitRange) {
            url += `/compare/${props.commitRange.replace(/^\.{3}/, 'HEAD...').replace(/\.{3}$/, '...HEAD')}`
        }
        // Add range or position path to the code host URL.
        if (props.range) {
            const rangeEndPrefix = externalURL.serviceKind === ExternalServiceKind.GITLAB ? '' : 'L'
            url += `#L${props.range.start.line}-${rangeEndPrefix}${props.range.end.line}`
        } else if (props.position) {
            url += `#L${props.position.line}`
        }
    }

    const TARGET_ID = 'go-to-code-host'

    return (
        <>
            <a
                className="nav-link test-go-to-code-host"
                // empty href is OK because we always set tabindex=0
                href={hijackLink ? '' : url}
                target="_blank"
                rel="noopener noreferrer"
                data-tooltip={`View on ${displayName}`}
                id={TARGET_ID}
                onClick={onClick}
                onAuxClick={onClick}
            >
                <Icon className="icon-inline" />
            </a>

            <InstallBrowserExtensionPopover
                url={url}
                toggle={toggle}
                isOpen={showPopover}
                serviceKind={externalURL.serviceKind}
                onClose={onClose}
                onRejection={onRejection}
                onClickInstall={onClickInstall}
                targetID={TARGET_ID}
            />
        </>
    )
}

export function serviceKindDisplayNameAndIcon(
    serviceKind: ExternalServiceKind | null
): { displayName: string; icon?: React.ComponentType<{ className?: string }> } {
    if (!serviceKind) {
        return { displayName: 'code host' }
    }

    switch (serviceKind) {
        case ExternalServiceKind.GITHUB:
            return { displayName: 'GitHub', icon: GithubIcon }
        case ExternalServiceKind.GITLAB:
            return { displayName: 'GitLab', icon: GitlabIcon }
        case ExternalServiceKind.BITBUCKETSERVER:
            return { displayName: 'Bitbucket Server', icon: BitbucketIcon }
        case ExternalServiceKind.PHABRICATOR:
            return { displayName: 'Phabricator', icon: PhabricatorIcon }
        case ExternalServiceKind.AWSCODECOMMIT:
            return { displayName: 'AWS CodeCommit' }
        default:
            return { displayName: upperFirst(toLower(serviceKind)) }
    }
}
