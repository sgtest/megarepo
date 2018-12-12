import { FileInfo } from '../code_intelligence'

export interface PageInfo extends Pick<FileInfo, 'repoName' | 'filePath' | 'rev'> {
    project: string
    repoSlug: string
}

const getFileInfoFromLink = (codeView: HTMLElement, linkSelector: string, fileInfoRegexp: RegExp) => {
    const rawViewLink = codeView.querySelector<HTMLAnchorElement>(linkSelector)
    if (!rawViewLink) {
        throw new Error(`could not find raw view link for code view (${linkSelector})`)
    }

    const url = new URL(rawViewLink.href)

    const host = window.location.hostname

    const path = url.pathname

    const pathMatch = path.match(fileInfoRegexp)
    if (!pathMatch) {
        throw new Error('Unable to parse file information')
    }

    const project = pathMatch[1]
    const repoSlug = pathMatch[2]
    const filePath = pathMatch[3]

    // Looks like 'refs/heads/<rev>'
    const at = url.searchParams.get('at')
    if (!at) {
        throw new Error('No `at` query param found')
    }

    const atMatch = at.match(/refs\/heads\/(.*?)$/)

    const rev = atMatch ? atMatch[1] : at

    return {
        repoName: [host, project, repoSlug].join('/'),
        filePath,
        rev,
        project,
        repoSlug,
    }
}

export const getFileInfoFromCodeView = (codeView: HTMLElement): PageInfo & Pick<FileInfo, 'commitID'> => {
    const { repoName, filePath, rev, project, repoSlug } = getFileInfoFromLink(
        codeView,
        'a.raw-view-link',
        // Looks like '/projects/<project>/repos/<repo name>/raw/<file path>'
        /\/projects\/(.*?)\/repos\/(.*?)\/raw\/(.*)$/
    )

    const commitLink = document.querySelector<HTMLElement>('a.commitid')
    if (!commitLink) {
        throw new Error('Could not find commit id link')
    }

    const commitID = commitLink.dataset.commitid!

    return {
        repoName,
        filePath,
        rev,
        commitID,
        project,
        repoSlug,
    }
}

const getFileInfoFromFilePathLink = (codeView: HTMLElement) => {
    const rawViewLink = codeView.querySelector<HTMLAnchorElement>('.breadcrumbs a.stub')
    if (!rawViewLink) {
        throw new Error('could not find raw view link for code view (.breadcrumbs a.stub)')
    }

    const url = new URL(rawViewLink.href)

    const host = window.location.hostname

    const path = url.pathname

    const pathMatch = path.match(/\/projects\/(.*?)\/repos\/(.*?)\/pull-requests\/(\d*)\//)
    if (!pathMatch) {
        throw new Error('Unable to parse file information')
    }

    const project = pathMatch[1]
    const repoSlug = pathMatch[2]

    const commitMatch = path.match(/\/commits\/(.*?)$/)

    const commitID = commitMatch ? commitMatch[1] : undefined

    let filePath = url.hash.replace(/^#/, '')
    filePath = filePath.replace(/\?.*$/, '')

    return {
        repoName: [host, project, repoSlug].join('/'),
        filePath,
        commitID,
        project,
        repoSlug,
    }
}

export interface PRPageInfo extends PageInfo {
    prID?: number
    commitID?: string // FileInfo.commitID is required but we won't always have it from the PR page DOM.
}

export const getPRInfoFromCodeView = (codeView: HTMLElement): PRPageInfo => {
    let repoName: string
    let filePath: string
    let project: string
    let repoSlug: string
    let commitID: string | undefined

    try {
        const info = getFileInfoFromLink(
            codeView,
            'a.source-view-link',
            // Looks like /projects/<project>/repos/<repo>/browse/<file path>?at=<rev>
            /\/projects\/(.*?)\/repos\/(.*?)\/browse\/(.*)$/
        )

        repoName = info.repoName
        filePath = info.filePath
        project = info.project
        repoSlug = info.repoSlug
    } catch (e) {
        const info = getFileInfoFromFilePathLink(codeView)

        repoName = info.repoName
        filePath = info.filePath
        project = info.project
        repoSlug = info.repoSlug
        commitID = info.commitID
    }

    const prIDMatch = window.location.pathname.match(/pull-requests\/(\d*?)\/(diff|overview|commits)/)

    if (!commitID) {
        const fromCommitLink = document.querySelector<HTMLAnchorElement>('.file-tree-header a.commitid')
        if (fromCommitLink) {
            commitID = fromCommitLink.dataset.commitid!
        }

        // Commit page
        if (!commitID) {
            const commitLink = document.querySelector<HTMLElement>('.commit-metadata-details .commitid')

            commitID = commitLink ? commitLink.dataset.commitid! : undefined
        }
    }

    return {
        repoName: repoName!,
        filePath: filePath!,
        commitID: commitID!,
        prID: prIDMatch ? parseInt(prIDMatch[1], 10) : undefined,
        project: project!,
        repoSlug: repoSlug!,
    }
}
