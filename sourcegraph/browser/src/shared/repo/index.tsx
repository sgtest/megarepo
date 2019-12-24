export interface DiffResolvedRevSpec {
    baseCommitID: string
    headCommitID: string
}

export interface OpenInSourcegraphProps {
    sourcegraphURL: string
    repoName: string
    rev: string
    filePath?: string
    commit?: {
        baseRev: string
        headRev: string
    }
    coords?: {
        line: number
        char: number
    }
    fragment?: 'references'
    query?: {
        search?: string
        diff?: {
            rev: string
        }
    }
    withModifierKey?: boolean
}

export interface OpenDiffInSourcegraphProps
    extends Pick<OpenInSourcegraphProps, Exclude<keyof OpenInSourcegraphProps, 'commit'>> {
    commit: {
        baseRev: string
        headRev: string
    }
}
