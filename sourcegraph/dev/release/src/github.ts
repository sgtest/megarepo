import Octokit from '@octokit/rest'
import { readLine } from './util'
import { promisify } from 'util'
import * as semver from 'semver'
import { mkdtemp as original_mkdtemp } from 'fs'
import * as os from 'os'
import * as path from 'path'
import execa from 'execa'
import commandExists from 'command-exists'
const mkdtemp = promisify(original_mkdtemp)

const formatDate = (date: Date): string => `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`

export async function ensureTrackingIssue({
    majorVersion,
    minorVersion,
    assignees,
    releaseDateTime,
    oneWorkingDayBeforeRelease,
    fourWorkingDaysBeforeRelease,
    fiveWorkingDaysBeforeRelease,
}: {
    majorVersion: string
    minorVersion: string
    assignees: string[]
    releaseDateTime: Date
    oneWorkingDayBeforeRelease: Date
    fourWorkingDaysBeforeRelease: Date
    fiveWorkingDaysBeforeRelease: Date
}): Promise<{ url: string; created: boolean }> {
    const octokit = await getAuthenticatedGitHubClient()
    const releaseIssueTemplate = await getContent(octokit, {
        owner: 'sourcegraph',
        repo: 'about',
        path: 'handbook/engineering/releases/release_issue_template.md',
    })
    const releaseIssueBody = releaseIssueTemplate
        .replace(/\$MAJOR/g, majorVersion)
        .replace(/\$MINOR/g, minorVersion)
        .replace(/\$RELEASE_DATE/g, formatDate(releaseDateTime))
        .replace(/\$FIVE_WORKING_DAYS_BEFORE_RELEASE/g, formatDate(fiveWorkingDaysBeforeRelease))
        .replace(/\$FOUR_WORKING_DAYS_BEFORE_RELEASE/g, formatDate(fourWorkingDaysBeforeRelease))
        .replace(/\$ONE_WORKING_DAY_BEFORE_RELEASE/g, formatDate(oneWorkingDayBeforeRelease))

    const milestoneTitle = `${majorVersion}.${minorVersion}`
    const milestones = await octokit.issues.listMilestonesForRepo({
        owner: 'sourcegraph',
        repo: 'sourcegraph',
        per_page: 100,
        direction: 'desc',
    })
    const milestone = milestones.data.filter(milestone => milestone.title === milestoneTitle)
    if (milestone.length === 0) {
        console.log(
            `Milestone ${JSON.stringify(
                milestoneTitle
            )} is closed or not found—you'll need to manually create it and add this issue to it.`
        )
    }

    return ensureIssue(octokit, {
        title: trackingIssueTitle(majorVersion, minorVersion),
        owner: 'sourcegraph',
        repo: 'sourcegraph',
        assignees,
        body: releaseIssueBody,
        milestone: milestone.length > 0 ? milestone[0].number : undefined,
    })
}

export async function ensurePatchReleaseIssue({
    version,
    assignees,
}: {
    version: semver.SemVer
    assignees: string[]
}): Promise<{ url: string; created: boolean }> {
    const octokit = await getAuthenticatedGitHubClient()
    const issueTemplate = await getContent(octokit, {
        owner: 'sourcegraph',
        repo: 'about',
        path: 'handbook/engineering/releases/patch_release_issue_template.md',
    })
    const issueBody = issueTemplate
        .replace(/\$MAJOR/g, version.major.toString())
        .replace(/\$MINOR/g, version.minor.toString())
        .replace(/\$PATCH/g, version.patch.toString())
    return ensureIssue(octokit, {
        title: `${version.version} patch release`,
        owner: 'sourcegraph',
        repo: 'sourcegraph',
        assignees,
        body: issueBody,
    })
}

async function getContent(
    octokit: Octokit,
    parameters: {
        owner: string
        repo: string
        path: string
    }
): Promise<string> {
    const response = await octokit.repos.getContents(parameters)
    if (Array.isArray(response.data)) {
        throw new TypeError(`${parameters.path} is a directory`)
    }
    return Buffer.from(response.data.content as string, 'base64').toString()
}

async function ensureIssue(
    octokit: Octokit,
    {
        title,
        owner,
        repo,
        assignees,
        body,
        milestone,
    }: {
        title: string
        owner: string
        repo: string
        assignees: string[]
        body: string
        milestone?: number
    }
): Promise<{ url: string; created: boolean }> {
    const url = await getIssueByTitle(octokit, title)
    if (url) {
        return { url, created: false }
    }
    const createdIssue = await octokit.issues.create({
        title,
        owner,
        repo,
        assignees,
        body,
        milestone,
    })
    return { url: createdIssue.data.html_url, created: true }
}

export async function listIssues(
    octokit: Octokit,
    query: string
): Promise<Octokit.SearchIssuesAndPullRequestsResponseItemsItem[]> {
    return (await octokit.search.issuesAndPullRequests({ per_page: 100, q: query })).data.items
}

export function trackingIssueTitle(major: string, minor: string): string {
    return `${major}.${minor} release tracking issue`
}

export async function getAuthenticatedGitHubClient(): Promise<Octokit> {
    const githubPAT = await readLine(
        'Enter a GitHub personal access token with "repo" scope (https://github.com/settings/tokens/new): ',
        '.secrets/github.txt'
    )
    const trimmedGithubPAT = githubPAT.trim()
    return new Octokit({ auth: trimmedGithubPAT })
}

export async function getIssueByTitle(octokit: Octokit, title: string): Promise<string | null> {
    const response = await octokit.search.issuesAndPullRequests({
        per_page: 100,
        q: `type:issue repo:sourcegraph/sourcegraph is:open ${JSON.stringify(title)}`,
    })

    const matchingIssues = response.data.items.filter(issue => issue.title === title)
    if (matchingIssues.length === 0) {
        return null
    }
    if (matchingIssues.length > 1) {
        throw new Error(`Multiple issues matched issue title ${JSON.stringify(title)}`)
    }
    return matchingIssues[0].html_url
}

export type EditFunc = (d: string) => void

export type Edit = string | EditFunc

export interface CreateBranchWithChangesOptions {
    owner: string
    repo: string
    base: string
    head: string
    commitMessage: string
    edits: Edit[]
    dryRun?: boolean
}

export interface ChangesetsOptions {
    requiredCommands: string[]
    changes: (Octokit.PullsCreateParams & CreateBranchWithChangesOptions)[]
    dryRun?: boolean
}

export async function createChangesets(options: ChangesetsOptions): Promise<void> {
    for (const command of options.requiredCommands) {
        try {
            await commandExists(command)
        } catch {
            throw new Error(`Required command ${command} does not exist`)
        }
    }
    for (const changeset of options.changes) {
        await createBranchWithChanges({ ...changeset, dryRun: options.dryRun })
        if (!options.dryRun) {
            const prURL = await createPR(changeset)
            console.log(`Pull request created: ${prURL}`)
        }
    }
}

async function createBranchWithChanges({
    owner,
    repo,
    base: baseRevision,
    head: headBranch,
    commitMessage,
    edits,
    dryRun,
}: CreateBranchWithChangesOptions): Promise<void> {
    const tmpdir = await mkdtemp(path.join(os.tmpdir(), `sg-release-${owner}-${repo}-`))
    console.log(`Created temp directory ${tmpdir}`)

    // Set up repository
    const setupScript = `set -ex

    git clone --depth 10 git@github.com:${owner}/${repo} || git clone --depth 10 https://github.com/${owner}/${repo};
    cd ./${repo};
    git checkout ${baseRevision};`
    await execa('bash', ['-c', setupScript], { stdio: 'inherit', cwd: tmpdir })
    const workdir = path.join(tmpdir, repo)

    // Apply edits
    for (const edit of edits) {
        switch (typeof edit) {
            case 'function':
                edit(workdir)
                break
            case 'string': {
                const editScript = `set -ex

                ${edit};`
                await execa('bash', ['-c', editScript], { stdio: 'inherit', cwd: workdir })
            }
        }
    }

    if (dryRun) {
        const showChangesScript = `set -ex

        git --no-pager diff;`
        await execa('bash', ['-c', showChangesScript], { stdio: 'inherit', cwd: workdir })
    } else {
        // Publish changes
        const publishScript = `set -ex

        git add :/;
        git commit -a -m ${JSON.stringify(commitMessage)};
        git push origin HEAD:${headBranch};`
        await execa('bash', ['-c', publishScript], { stdio: 'inherit', cwd: workdir })
    }
}

async function createPR(options: {
    owner: string
    repo: string
    head: string
    base: string
    title: string
    body?: string
}): Promise<string> {
    const octokit = await getAuthenticatedGitHubClient()
    const response = await octokit.pulls.create(options)
    return response.data.html_url
}
