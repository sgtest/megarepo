import * as React from 'react'

import { parseRepoRevision } from '@sourcegraph/shared/src/util/url'
import { useIsTruncated } from '@sourcegraph/wildcard'

import { useOpenSearchResultsContext } from '../MatchHandlersContext'

/**
 * Returns the friendly display form of the repository name (e.g., removing "github.com/").
 */
export function displayRepoName(repoName: string): string {
    let parts = repoName.split('/')
    if (parts.length >= 3 && parts[0].includes('.')) {
        parts = parts.slice(1) // remove hostname from repo name (reduce visual noise)
    }
    return parts.join('/')
}

/**
 * Splits the repository name into the dir and base components.
 */
export function splitPath(path: string): [string, string] {
    const components = path.split('/')
    return [components.slice(0, -1).join('/'), components[components.length - 1]]
}

interface Props {
    repoName: string
    repoURL: string
    filePath: string
    fileURL: string
    repoDisplayName?: string
    className?: string
}

/**
 * A link to a repository or a file within a repository, formatted as "repo" or "repo > file". Unless you
 * absolutely need breadcrumb-like behavior, use this instead of FilePathBreadcrumb.
 */
export const RepoFileLink: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    repoDisplayName,
    repoName,
    repoURL,
    filePath,
    className,
}) => {
    /**
     * Use the custom hook useIsTruncated to check if overflow: ellipsis is activated for the element
     * We want to do it on mouse enter as browser window size might change after the element has been
     * loaded initially
     */
    const [titleReference, truncated, checkTruncation] = useIsTruncated()

    const [fileBase, fileName] = splitPath(filePath)

    const { openRepo, openFile } = useOpenSearchResultsContext()

    const getRepoAndRevision = (): { repoName: string; revision: string | undefined } => {
        // Example: `/github.com/sourcegraph/sourcegraph@main`
        const indexOfSeparator = repoURL.indexOf('/-/')
        let repoRevision: string
        if (indexOfSeparator === -1) {
            repoRevision = repoURL // the whole string
        } else {
            repoRevision = repoURL.slice(0, indexOfSeparator) // the whole string leading up to the separator (allows revision to be multiple path parts)
        }
        let { repoName, revision } = parseRepoRevision(repoRevision)
        // Remove leading slash
        if (repoName.startsWith('/')) {
            repoName = repoName.slice(1)
        }
        return { repoName, revision }
    }

    const onRepoClick = (): void => {
        const { repoName, revision } = getRepoAndRevision()

        openRepo({
            repository: repoName,
            branches: revision ? [revision] : undefined,
        })
    }

    const onFileClick = (): void => {
        const { repoName, revision } = getRepoAndRevision()
        openFile(repoName, { path: filePath, revision })
    }

    return (
        <div
            ref={titleReference}
            className={className}
            onMouseEnter={checkTruncation}
            data-tooltip={truncated ? (fileBase ? `${fileBase}/${fileName}` : fileName) : null}
        >
            <button onClick={onRepoClick} type="button" className="btn btn-text-link">
                {repoDisplayName || displayRepoName(repoName)}
            </button>{' '}
            ›{' '}
            <button onClick={onFileClick} type="button" className="btn btn-text-link">
                {fileBase ? `${fileBase}/` : null}
                <strong>{fileName}</strong>
            </button>
        </div>
    )
}
