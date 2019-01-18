import { GitHubBlobUrl, GitHubMode, GitHubPullUrl, GitHubRepositoryUrl, GitHubURL } from '.'
import { DiffResolvedRevSpec } from '../../shared/repo'

/**
 * getFileContainers returns the elements on the page which should be marked
 * up with tooltips & links:
 *
 * 1. blob view: a single file
 * 2. commit view: one or more file diffs
 * 3. PR conversation view: snippets with inline comments
 * 4. PR unified/split view: one or more file diffs
 */
export function getFileContainers(): HTMLCollectionOf<HTMLElement> {
    return document.getElementsByClassName('file') as HTMLCollectionOf<HTMLElement>
}

/**
 * Creates the mount element for the CodeViewToolbar.
 */
export function createCodeViewToolbarMount(fileContainer: HTMLElement): HTMLElement {
    const className = 'sourcegraph-app-annotator'
    const existingMount = fileContainer.querySelector('.' + className) as HTMLElement
    if (existingMount) {
        return existingMount
    }

    const mountEl = document.createElement('div')
    mountEl.style.display = 'inline-flex'
    mountEl.style.verticalAlign = 'middle'
    mountEl.style.alignItems = 'center'
    mountEl.className = className

    const fileActions = fileContainer.querySelector('.file-actions')
    if (!fileActions) {
        throw new Error(
            "File actions not found. Make sure you aren't trying to create " +
                "a toolbar mount for a code snippet that shouldn't have one"
        )
    }

    const buttonGroup = fileActions.querySelector('.BtnGroup')
    if (buttonGroup && buttonGroup.parentNode && !fileContainer.querySelector('.show-file-notes')) {
        // blob view
        buttonGroup.parentNode.insertBefore(mountEl, buttonGroup)
    } else {
        // commit & pull request view
        const note = fileContainer.querySelector('.show-file-notes')
        if (!note || !note.parentNode) {
            throw new Error('cannot find toolbar mount location')
        }
        note.parentNode.insertBefore(mountEl, note.nextSibling)
    }

    return mountEl
}

/**
 * getDeltaFileName returns the path of the file container. It assumes
 * the file container is for a diff (i.e. a commit or pull request view).
 */
export function getDeltaFileName(container: HTMLElement): { headFilePath: string; baseFilePath: string | null } {
    const info = container.querySelector('.file-info') as HTMLElement

    if (info.title) {
        // for PR conversation snippets
        return getPathNamesFromElement(info)
    } else {
        const link = info.querySelector('a') as HTMLElement
        return getPathNamesFromElement(link)
    }
}

function getPathNamesFromElement(element: HTMLElement): { headFilePath: string; baseFilePath: string | null } {
    const elements = element.title.split(' → ')
    if (elements.length > 1) {
        return { headFilePath: elements[1], baseFilePath: elements[0] }
    }
    return { headFilePath: elements[0], baseFilePath: elements[0] }
}

/**
 * isDomSplitDiff returns if the current view shows diffs with split (vs. unified) view.
 */
export function isDomSplitDiff(): boolean {
    const { isDelta, isPullRequest } = parseURL()
    if (!isDelta) {
        return false
    }

    if (isPullRequest) {
        const headerBar = document.getElementsByClassName('float-right pr-review-tools')
        if (!headerBar || headerBar.length !== 1) {
            return false
        }

        const diffToggles = headerBar[0].getElementsByClassName('BtnGroup')
        const disabledToggle = diffToggles[0].getElementsByTagName('A')[0] as HTMLAnchorElement
        return (
            (disabledToggle && !disabledToggle.href.includes('diff=split')) ||
            !!document.querySelector('.file-diff-split')
        )
    } else {
        // delta for a commit view
        const headerBar = document.getElementsByClassName('details-collapse table-of-contents js-details-container')
        if (!headerBar || headerBar.length !== 1) {
            return false
        }

        const diffToggles = headerBar[0].getElementsByClassName('BtnGroup float-right')
        const selectedToggle = diffToggles[0].querySelector('.selected') as HTMLAnchorElement
        return (
            (selectedToggle && selectedToggle.href.includes('diff=split')) ||
            !!document.querySelector('.file-diff-split')
        )
    }
}

/**
 * getDiffResolvedRev returns the base and head revision SHA, or null for non-diff views.
 */
export function getDiffResolvedRev(): DiffResolvedRevSpec | null {
    const { isDelta, isCommit, isPullRequest, isCompare } = parseURL()
    if (!isDelta) {
        return null
    }

    let baseCommitID = ''
    let headCommitID = ''
    const fetchContainers = document.getElementsByClassName(
        'js-socket-channel js-updatable-content js-pull-refresh-on-pjax'
    )
    if (isPullRequest) {
        if (fetchContainers && fetchContainers.length === 1) {
            // tslint:disable-next-line
            for (let i = 0; i < fetchContainers.length; ++i) {
                // for conversation view of pull request
                const el = fetchContainers[i] as HTMLElement
                const url = el.getAttribute('data-url')
                if (!url) {
                    continue
                }

                const urlSplit = url.split('?')
                const query = urlSplit[1]
                const querySplit = query.split('&')
                for (const kv of querySplit) {
                    const kvSplit = kv.split('=')
                    const k = kvSplit[0]
                    const v = kvSplit[1]
                    if (k === 'base_commit_oid') {
                        baseCommitID = v
                    }
                    if (k === 'end_commit_oid') {
                        headCommitID = v
                    }
                }
            }
        } else {
            // Last-ditch: look for inline comment form input which has base/head on it.
            const baseInput = document.querySelector(`input[name="comparison_start_oid"]`)
            if (baseInput) {
                baseCommitID = (baseInput as HTMLInputElement).value
            }
            const headInput = document.querySelector(`input[name="comparison_end_oid"]`)
            if (headInput) {
                headCommitID = (headInput as HTMLInputElement).value
            }
        }
    } else if (isCommit) {
        const shaContainer = document.querySelectorAll('.sha-block')
        if (shaContainer && shaContainer.length === 2) {
            const baseShaEl = shaContainer[0].querySelector('a')
            if (baseShaEl) {
                // e.g "https://github.com/gorilla/mux/commit/0b13a922203ebdbfd236c818efcd5ed46097d690"
                baseCommitID = baseShaEl.href.split('/').slice(-1)[0]
            }
            const headShaEl = shaContainer[1].querySelector('span.sha') as HTMLElement
            if (headShaEl) {
                headCommitID = headShaEl.innerHTML
            }
        }
    } else if (isCompare) {
        const resolvedDiffSpec = getResolvedDiffForCompare()
        if (resolvedDiffSpec) {
            return resolvedDiffSpec
        }
    }

    if (baseCommitID === '' || headCommitID === '') {
        return getDiffResolvedRevFromPageSource(document.documentElement!.innerHTML)
    }
    return { baseCommitID, headCommitID }
}

function getResolvedDiffForCompare(): DiffResolvedRevSpec | undefined {
    const branchElements = document.querySelectorAll('.commitish-suggester span.js-select-button') as NodeListOf<
        HTMLSpanElement
    >
    if (branchElements && branchElements.length === 2) {
        return { baseCommitID: branchElements[0].innerText, headCommitID: branchElements[1].innerText }
    }
}

function getDiffResolvedRevFromPageSource(pageSource: string): DiffResolvedRevSpec | null {
    const { isPullRequest } = parseURL()
    if (!isPullRequest) {
        return null
    }
    const baseShaComment = '<!-- base sha1: &quot;'
    const baseIndex = pageSource.indexOf(baseShaComment)

    if (baseIndex === -1) {
        return null
    }

    const headShaComment = '<!-- head sha1: &quot;'
    const headIndex = pageSource.indexOf(headShaComment, baseIndex)
    if (headIndex === -1) {
        return null
    }

    const baseCommitID = pageSource.substr(baseIndex + baseShaComment.length, 40)
    const headCommitID = pageSource.substr(headIndex + headShaComment.length, 40)
    return {
        baseCommitID,
        headCommitID,
    }
}

const GITHUB_BLOB_REGEX = /^(https?):\/\/(github.com)\/([A-Za-z0-9_]+)\/([A-Za-z0-9-]+)\/blob\/([^#]*)(#L[0-9]+)?/i
const GITHUB_PULL_REGEX = /^(https?):\/\/(github.com)\/([A-Za-z0-9_]+)\/([A-Za-z0-9-]+)\/pull\/([0-9]+)(\/(commits|files))?/i
const COMMIT_HASH_REGEX = /^([0-9a-f]{40})/i
export function getGitHubState(url: string): GitHubBlobUrl | GitHubPullUrl | GitHubRepositoryUrl | null {
    const blobMatch = GITHUB_BLOB_REGEX.exec(url)
    if (blobMatch) {
        const match = {
            protocol: blobMatch[1],
            hostname: blobMatch[2],
            org: blobMatch[3],
            repo: blobMatch[4],
            revAndPath: blobMatch[5],
            lineNumber: blobMatch[6],
        }
        const rev = getRevOrBranch(match.revAndPath)
        if (!rev) {
            return null
        }
        const filePath = match.revAndPath.replace(rev + '/', '')
        return {
            mode: GitHubMode.Blob,
            owner: match.org,
            ghRepoName: match.repo,
            revAndPath: match.revAndPath,
            lineNumber: match.lineNumber,
            rev,
            filePath,
        }
    }
    const pullMatch = GITHUB_PULL_REGEX.exec(url)
    if (pullMatch) {
        const match = {
            protocol: pullMatch[1],
            hostname: pullMatch[2],
            org: pullMatch[3],
            repo: pullMatch[4],
            id: pullMatch[5],
            view: pullMatch[7],
        }
        const numId: number = parseInt(match.id, 10)
        if (isNaN(numId)) {
            console.error(`match.id ${match.id} is parsing to NaN`)
            return null
        }
        return {
            mode: GitHubMode.PullRequest,
            ghRepoName: match.repo,
            owner: match.org,
            view: match.view,
            rev: '',
            id: numId,
        }
    }
    const parsed = parseURL()
    if (parsed && parsed.ghRepoName && parsed.repoName && parsed.user) {
        return {
            mode: GitHubMode.Repository,
            owner: parsed.user,
            ghRepoName: parsed.ghRepoName,
            rev: parsed.rev,
            filePath: parsed.filePath,
        }
    }

    return null
}

function getBranchName(): string | null {
    const branchButtons = document.getElementsByClassName('btn btn-sm select-menu-button js-menu-target css-truncate')
    if (branchButtons.length === 0) {
        return null
    }
    // if the branch is a long name, it appears in the title of this element
    // I'm not kidding, so dumb...
    if ((branchButtons[0] as HTMLElement).title) {
        return (branchButtons[0] as HTMLElement).title
    }
    const innerButtonEls = (branchButtons[0] as HTMLElement).getElementsByClassName(
        'js-select-button css-truncate-target'
    )
    if (innerButtonEls.length === 0) {
        return null
    }
    // otherwise, the branch name is fully rendered in the button
    return (innerButtonEls[0] as HTMLElement).innerText as string
}

function getRevOrBranch(revAndPath: string): string | null {
    const matchesCommit = COMMIT_HASH_REGEX.exec(revAndPath)
    if (matchesCommit) {
        return matchesCommit[1].substring(0, 40)
    }
    const branch = getBranchName()
    if (!branch) {
        return null
    }
    if (!revAndPath.startsWith(branch)) {
        console.error(`branch and path is ${revAndPath}, and branch is ${branch}`)
        return null
    }
    return branch
}

export function parseURL(loc: Location = window.location): GitHubURL {
    // TODO(john): this method has problems handling branch revisions with "/" character.
    // TODO(john): this all needs unit testing!

    let user: string | undefined
    let ghRepoName: string | undefined // in "github.com/foo/bar", just "bar"
    let repoName: string | undefined
    let rev: string | undefined
    let filePath: string | undefined

    const urlsplit = loc.pathname.slice(1).split('/')
    user = urlsplit[0]
    ghRepoName = urlsplit[1]

    let revParts = 1 // a revision may have "/" chars, in which case we consume multiple parts;
    if ((urlsplit[3] && (urlsplit[2] === 'tree' || urlsplit[2] === 'blob')) || urlsplit[2] === 'commit') {
        const currBranch = getBranchName()
        if (currBranch) {
            revParts = currBranch.split('/').length
        }
        rev = urlsplit.slice(3, 3 + revParts).join('/')
    }
    if (urlsplit[2] === 'blob') {
        filePath = urlsplit.slice(3 + revParts).join('/')
    }
    if (user && ghRepoName) {
        repoName = `${window.location.host}/${user}/${ghRepoName}`
    } else {
        repoName = ''
    }

    const isCompare = urlsplit[2] === 'compare'
    const isPullRequest = urlsplit[2] === 'pull'
    const isCommit = urlsplit[2] === 'commit'
    const isDelta = isPullRequest || isCommit || isCompare
    const isCodePage = urlsplit[2] === 'blob' || urlsplit[2] === 'tree'

    const hash = parseGitHubHash(loc.hash)
    const position = hash ? { line: hash.startLine, character: 0 } : undefined

    return {
        user,
        repoName,
        rev,
        filePath,
        ghRepoName,
        isDelta,
        isPullRequest,
        position,
        isCommit,
        isCodePage,
        isCompare,
    }
}

/**
 * Parses the GitHub URL hash, such as "#L23-L28" in
 * https://github.com/ReactTraining/react-router/blob/master/packages/react-router/modules/Router.js#L23-L28.
 *
 * This hash has a slightly different format from Sourcegraph URL hashes. GitHub hashes do not support specifying
 * the character on a line, and GitHub hashes duplicate the "L" before the range end line number.
 */
export function parseGitHubHash(hash: string): { startLine: number; endLine?: number } | undefined {
    const m = hash.match(/^#?L(\d+)(?:-L(\d+))?/)
    if (!m) {
        return undefined
    }
    const startLine = parseInt(m[1], 10)
    const endLine = m[2] ? parseInt(m[2], 10) : undefined
    return { startLine, endLine }
}
