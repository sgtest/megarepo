import { GitHubBlobUrl, GitHubMode, GitHubPullUrl, GitHubRepositoryUrl, GitHubURL } from '.'
import { CodeCell, DiffRepoRev, DiffResolvedRevSpec, MaybeDiffSpec } from '../../shared/repo'
import { parseHash } from '../../shared/util/url'

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
 * createBlobAnnotatorMount creates a <div> element and adds it to the DOM
 * where the BlobAnnotator component should be mounted.
 */
export function createBlobAnnotatorMount(fileContainer: HTMLElement, isBase?: boolean): HTMLElement | null {
    if (isInlineCommentContainer(fileContainer)) {
        return null
    }

    const className = 'sourcegraph-app-annotator' + (isBase ? '-base' : '')
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
        // E.g. snippets on the PR conversation view.
        return null
    }
    const buttonGroup = fileActions.querySelector('.BtnGroup')
    if (buttonGroup && buttonGroup.parentNode && !fileContainer.querySelector('.show-file-notes')) {
        // blob view
        buttonGroup.parentNode.insertBefore(mountEl, buttonGroup)
    } else {
        // commit & pull request view
        const note = fileContainer.querySelector('.show-file-notes')
        if (!note || !note.parentNode) {
            throw new Error('cannot locate BlobAnnotator injection site')
        }
        note.parentNode.insertBefore(mountEl, note.nextSibling)
    }

    return mountEl
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

export function isInlineCommentContainer(file: HTMLElement): boolean {
    return file.classList.contains('inline-review-comment')
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
        return getDiffResolvedRevFromPageSource(document.documentElement.innerHTML)
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

/**
 * getDiffRepoRev returns the base and head branches & URIs, or null for non-diff views.
 */
export function getDiffRepoRev(): DiffRepoRev | null {
    const { repoPath, isDelta, isPullRequest, isCommit, isCompare } = parseURL()
    if (!isDelta) {
        return null
    }

    let baseRev = ''
    let headRev = ''
    let baseRepoPath = ''
    let headRepoPath = ''
    if (isPullRequest) {
        const branches = document.querySelectorAll('.commit-ref')
        baseRev = (branches[0] as any).title
        headRev = (branches[1] as any).title

        if (baseRev.includes(':')) {
            const baseSplit = baseRev.split(':')
            baseRev = baseSplit[1]
            baseRepoPath = `${window.location.host}/${baseSplit[0]}`
        } else {
            baseRev = repoPath as string
        }
        if (headRev.includes(':')) {
            const headSplit = headRev.split(':')
            headRev = headSplit[1]
            headRepoPath = `${window.location.host}/${headSplit[0]}`
        } else {
            headRepoPath = repoPath as string
        }
    } else if (isCommit) {
        let branchEl = document.querySelector('li.branch') as HTMLElement
        if (branchEl) {
            branchEl = branchEl.querySelector('a') as HTMLElement
        }
        if (branchEl) {
            baseRev = branchEl.innerHTML
            headRev = branchEl.innerHTML
        } else {
            const headCommitEl = document.querySelector('[name="commit_id"]') as HTMLInputElement
            if (headCommitEl) {
                headRev = headCommitEl.value
            }
            const baseCommitEl = document.querySelector('.sha-block > .sha') as HTMLAnchorElement
            if (baseCommitEl) {
                baseRev = baseCommitEl.innerText
            }
        }
        baseRepoPath = repoPath as string
        headRepoPath = repoPath as string
    } else if (isCompare) {
        const resolvedDiffSpec = getResolvedDiffForCompare()
        if (resolvedDiffSpec) {
            baseRev = resolvedDiffSpec.baseCommitID
            headRev = resolvedDiffSpec.headCommitID
        }
        const forkElements = document.querySelectorAll('.fork-suggester span.js-select-button') as NodeListOf<
            HTMLSpanElement
        >
        if (forkElements && forkElements.length === 2) {
            baseRepoPath = `${window.location.host}/${forkElements[0].innerText}`
            headRepoPath = `${window.location.host}/${forkElements[1].innerText}`
        }
    }

    if (baseRev === '' || headRev === '' || baseRepoPath === '' || headRepoPath === '') {
        return null
    }
    return { baseRev, headRev, baseRepoPath, headRepoPath }
}

/**
 * getCodeCellsForAnnotation code cells which should be annotated
 */
export function getCodeCells(table: HTMLTableElement, opt: MaybeDiffSpec): CodeCell[] {
    const cells: CodeCell[] = []
    for (let i = 0; i < table.rows.length; ++i) {
        const row = table.rows[i]

        // Inline comments can be on
        if (row.className.includes('inline-comments')) {
            continue
        }

        let line: number // line number of the current line
        let codeCell: HTMLTableDataCellElement // the actual cell that has code inside; each row contains multiple columns
        let isAddition: boolean | undefined
        let isDeletion: boolean | undefined
        if (opt.isDelta) {
            if ((opt.isSplitDiff && row.cells.length !== 4) || (!opt.isSplitDiff && row.cells.length !== 3)) {
                // for "diff expander" lines
                continue
            }

            let lineCell: HTMLTableDataCellElement
            if (opt.isSplitDiff) {
                lineCell = opt.isBase ? row.cells[0] : row.cells[2]
            } else {
                lineCell = opt.isBase ? row.cells[0] : row.cells[1]
            }

            if (opt.isSplitDiff) {
                codeCell = opt.isBase ? row.cells[1] : row.cells[3]
            } else {
                codeCell = row.cells[2]
            }

            if (!codeCell) {
                console.error(`missing code cell at row ${i}`, table)
                continue
            }

            if (codeCell.className.includes('blob-code-empty')) {
                // for split diffs, this class represents "empty" ranges for one side of the diff
                continue
            }

            isAddition = codeCell.className.includes('blob-code-addition')
            isDeletion = codeCell.className.includes('blob-code-deletion')

            // additions / deletions should be annotated with the correct revision;
            // unmodified parts should only be annotated once;
            // head is preferred over base for unmodified parts because of the ?w=1 parameter
            if (!isAddition && !isDeletion && opt.isBase && !opt.isSplitDiff) {
                continue
            }
            if (isDeletion && !opt.isBase) {
                continue
            }
            if (isAddition && opt.isBase) {
                continue
            }

            const lineData = lineCell.getAttribute('data-line-number') as string
            if (lineData === '...') {
                // row before line "1" on diff views
                continue
            }
            line = parseInt(lineData, 10)
        } else {
            const lineCell = row.cells[0]
            if (!lineCell) {
                continue
            }
            // Some blob views do not user the data-line-number attribute and instead use a specific class.
            if (lineCell.className === 'blob-num') {
                line = parseInt(lineCell.innerText, 10)
            } else {
                line = parseInt(lineCell.getAttribute('data-line-number') as string, 10)
            }
            codeCell = row.cells[1]
        }
        if (!codeCell) {
            continue
        }

        const innerCode = codeCell.querySelector('.blob-code-inner') // ignore extraneous inner elements, like "comment" button on diff views
        cells.push({
            cell: (innerCode || codeCell) as HTMLElement,
            eventHandler: codeCell, // allways the TD element
            line,
            isAddition,
            isDeletion,
        })
    }

    return cells
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
            repoName: match.repo,
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
            repoName: match.repo,
            owner: match.org,
            view: match.view,
            rev: '',
            id: numId,
        }
    }
    const parsed = parseURL()
    if (parsed && parsed.repoName && parsed.repoPath && parsed.user) {
        return {
            mode: GitHubMode.Repository,
            owner: parsed.user,
            repoName: parsed.repoName,
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
    let repoName: string | undefined
    let repoPath: string | undefined
    let rev: string | undefined
    let filePath: string | undefined

    const urlsplit = loc.pathname.slice(1).split('/')
    user = urlsplit[0]
    repoName = urlsplit[1]

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
    if (user && repoName) {
        repoPath = `${window.location.host}/${user}/${repoName}`
    } else {
        repoPath = ''
    }

    const isCompare = urlsplit[2] === 'compare'
    const isPullRequest = urlsplit[2] === 'pull'
    const isCommit = urlsplit[2] === 'commit'
    const isDelta = isPullRequest || isCommit || isCompare
    const isCodePage = urlsplit[2] === 'blob' || urlsplit[2] === 'tree'

    const hash = parseHash(loc.hash)
    const position = hash.line ? { line: hash.line, character: hash.character || 0 } : undefined

    return {
        user,
        repoName,
        rev,
        filePath,
        repoPath,
        isDelta,
        isPullRequest,
        position,
        isCommit,
        isCodePage,
        isCompare,
    }
}

// Code Comments
export function getCodeCommentContainers(): HTMLCollectionOf<HTMLElement> {
    return document.getElementsByClassName('js-comment-body') as HTMLCollectionOf<HTMLElement>
}

// Repository search
export function getRepoCodeSearchContainers(): HTMLCollectionOf<HTMLElement> {
    return document.getElementsByClassName('code-list-item') as HTMLCollectionOf<HTMLElement>
}
