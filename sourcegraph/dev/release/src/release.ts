import { ensureEvent, getClient, EventOptions } from './google-calendar'
import { postMessage } from './slack'
import {
    ensureTrackingIssue,
    getAuthenticatedGitHubClient,
    listIssues,
    getIssueByTitle,
    trackingIssueTitle,
    ensurePatchReleaseIssue,
    createChangesets,
    CreatedChangeset,
} from './github'
import * as changelog from './changelog'
import * as campaigns from './campaigns'
import { formatDate, timezoneLink, readLine, getWeekNumber } from './util'
import * as persistedConfig from './config.json'
import { addMinutes, isWeekend, eachDayOfInterval, addDays, subDays } from 'date-fns'
import * as semver from 'semver'
import execa from 'execa'
import { readFileSync, writeFileSync } from 'fs'
import * as path from 'path'
import commandExists from 'command-exists'

const sed = process.platform === 'linux' ? 'sed' : 'gsed'
interface Config {
    teamEmail: string

    captainSlackUsername: string
    captainGitHubUsername: string

    previousRelease: string
    upcomingRelease: string

    releaseDateTime: string
    oneWorkingDayBeforeRelease: string
    fourWorkingDaysBeforeRelease: string
    fiveWorkingDaysBeforeRelease: string

    slackAnnounceChannel: string

    dryRun: {
        changesets?: boolean
        trackingIssues?: boolean
    }
}

/**
 * Convenience function for getting relevant configured releases as semver.SemVer
 *
 * It prompts for a confirmation of the `upcomingRelease` that is cached for a week.
 */
async function releaseVersions(
    config: Config
): Promise<{
    previous: semver.SemVer
    upcoming: semver.SemVer
}> {
    const parseOptions: semver.Options = { loose: false }
    const parsedPrevious = semver.parse(config.previousRelease, parseOptions)
    if (!parsedPrevious) {
        throw new Error(`config.previousRelease '${config.previousRelease}' is not valid semver`)
    }
    const parsedUpcoming = semver.parse(config.upcomingRelease, parseOptions)
    if (!parsedUpcoming) {
        throw new Error(`config.upcomingRelease '${config.upcomingRelease}' is not valid semver`)
    }

    // Verify the configured upcoming release. The response is cached and expires in a
    // week, after which the captain is required to confirm again.
    const now = new Date()
    const cachedVersion = `.secrets/current_release_${now.getUTCFullYear()}_${getWeekNumber(now)}.txt`
    const confirmVersion = await readLine(
        `Please confirm the upcoming release version (configured: '${config.upcomingRelease}'): `,
        cachedVersion
    )
    const parsedConfirmed = semver.parse(confirmVersion, parseOptions)
    if (!parsedConfirmed) {
        throw new Error(`Provided version '${confirmVersion}' is not valid semver (in ${cachedVersion})`)
    }
    if (semver.neq(parsedConfirmed, parsedUpcoming)) {
        throw new Error(
            `Provided version '${confirmVersion}' and config.upcomingRelease '${config.upcomingRelease}' to not match - please update the release configuration`
        )
    }

    const versions = {
        previous: parsedPrevious,
        upcoming: parsedUpcoming,
    }
    console.log(`Using versions: { upcoming: ${versions.upcoming.format()}, previous: ${versions.previous.format()} }`)
    return versions
}

type StepID =
    | 'help'
    // release tracking
    | 'tracking:release-timeline'
    | 'tracking:release-issue'
    | 'tracking:patch-issue'
    // branch cut
    | 'changelog:cut'
    // release
    | 'release:status'
    | 'release:create-candidate'
    | 'release:stage'
    | 'release:add-to-campaign'
    | 'release:close'
    // testing
    | '_test:google-calendar'
    | '_test:slack'
    | '_test:campaign-create-from-changes'

interface Step {
    id: StepID
    run?: ((config: Config, ...args: string[]) => Promise<void>) | ((config: Config, ...args: string[]) => void)
    argNames?: string[]
}

const steps: Step[] = [
    {
        id: 'help',
        run: () => {
            console.error('Steps are:')
            console.error(
                steps
                    .filter(({ id }) => !id.startsWith('_'))
                    .map(
                        ({ id, argNames }) =>
                            '\t' +
                            id +
                            (argNames && argNames.length > 0
                                ? ' ' + argNames.map(argumentName => `<${argumentName}>`).join(' ')
                                : '')
                    )
                    .join('\n')
            )
        },
    },
    {
        id: 'tracking:release-timeline',
        run: async config => {
            const googleCalendar = await getClient()
            const { upcoming: release } = await releaseVersions(config)
            const events: EventOptions[] = [
                {
                    title: 'Release captain: prepare for branch cut (5 working days until release)',
                    description: 'See the release tracking issue for TODOs',
                    startDateTime: new Date(config.fiveWorkingDaysBeforeRelease).toISOString(),
                    endDateTime: addMinutes(new Date(config.fiveWorkingDaysBeforeRelease), 1).toISOString(),
                },
                {
                    title: 'Release captain: branch cut (4 working days until release)',
                    description: 'See the release tracking issue for TODOs',
                    startDateTime: new Date(config.fourWorkingDaysBeforeRelease).toISOString(),
                    endDateTime: addMinutes(new Date(config.fourWorkingDaysBeforeRelease), 1).toISOString(),
                },
                ...eachDayOfInterval({
                    start: addDays(new Date(config.fourWorkingDaysBeforeRelease), 1),
                    end: subDays(new Date(config.oneWorkingDayBeforeRelease), 1),
                })
                    .filter(date => !isWeekend(date))
                    .map(date => ({
                        title: 'Release captain: cut new release candidate',
                        description: 'See release tracking issue for TODOs',
                        startDateTime: date.toISOString(),
                        endDateTime: addMinutes(date, 1).toISOString(),
                    })),
                {
                    title: 'Release captain: tag final release (1 working day before release)',
                    description: 'See the release tracking issue for TODOs',
                    startDateTime: new Date(config.oneWorkingDayBeforeRelease).toISOString(),
                    endDateTime: addMinutes(new Date(config.oneWorkingDayBeforeRelease), 1).toISOString(),
                },
                {
                    title: `Cut release branch ${release.major}.${release.minor}`,
                    description: '(This is not an actual event to attend, just a calendar marker.)',
                    anyoneCanAddSelf: true,
                    attendees: [config.teamEmail],
                    startDateTime: new Date(config.fourWorkingDaysBeforeRelease).toISOString(),
                    endDateTime: addMinutes(new Date(config.fourWorkingDaysBeforeRelease), 1).toISOString(),
                },
                {
                    title: `Release Sourcegraph ${release.major}.${release.minor}`,
                    description: '(This is not an actual event to attend, just a calendar marker.)',
                    anyoneCanAddSelf: true,
                    attendees: [config.teamEmail],
                    startDateTime: new Date(config.releaseDateTime).toISOString(),
                    endDateTime: addMinutes(new Date(config.releaseDateTime), 1).toISOString(),
                },
            ]

            for (const event of events) {
                console.log(`Create calendar event: ${event.title}: ${event.startDateTime || 'undefined'}`)
                await ensureEvent(event, googleCalendar)
            }
        },
    },
    {
        id: 'tracking:release-issue',
        run: async (config: Config) => {
            const {
                releaseDateTime,
                captainGitHubUsername,
                oneWorkingDayBeforeRelease,
                fourWorkingDaysBeforeRelease,
                fiveWorkingDaysBeforeRelease,

                captainSlackUsername,
                slackAnnounceChannel,
                dryRun,
            } = config
            const { upcoming: release } = await releaseVersions(config)

            // Create issue
            const { url, created } = await ensureTrackingIssue({
                version: release,
                assignees: [captainGitHubUsername],
                releaseDateTime: new Date(releaseDateTime),
                oneWorkingDayBeforeRelease: new Date(oneWorkingDayBeforeRelease),
                fourWorkingDaysBeforeRelease: new Date(fourWorkingDaysBeforeRelease),
                fiveWorkingDaysBeforeRelease: new Date(fiveWorkingDaysBeforeRelease),
                dryRun: dryRun.trackingIssues || false,
            })
            if (url) {
                console.log(created ? `Created tracking issue ${url}` : `Tracking issue already exists: ${url}`)
            }

            // Announce issue if issue does not already exist
            if (created) {
                // Slack markdown links
                const majorMinor = `${release.major}.${release.minor}`
                const branchCutDate = new Date(fourWorkingDaysBeforeRelease)
                const branchCutDateString = `<${timezoneLink(branchCutDate, `${majorMinor} branch cut`)}|${formatDate(
                    branchCutDate
                )}>`
                const releaseDate = new Date(releaseDateTime)
                const releaseDateString = `<${timezoneLink(releaseDate, `${majorMinor} release`)}|${formatDate(
                    releaseDate
                )}>`
                await postMessage(
                    `*${majorMinor} Release*

:captain: Release captain: @${captainSlackUsername}
:pencil: Tracking issue: ${url}
:spiral_calendar_pad: Key dates:
* Branch cut: ${branchCutDateString}
* Release: ${releaseDateString}`,
                    slackAnnounceChannel
                )
                console.log(`Posted to Slack channel ${slackAnnounceChannel}`)
            }
        },
    },
    {
        id: 'tracking:patch-issue',
        run: async config => {
            const { captainGitHubUsername, slackAnnounceChannel, dryRun } = config
            const { upcoming: release } = await releaseVersions(config)

            // Create issue
            const { url, created } = await ensurePatchReleaseIssue({
                version: release,
                assignees: [captainGitHubUsername],
                dryRun: dryRun.trackingIssues || false,
            })
            if (url) {
                console.log(created ? `Created tracking issue ${url}` : `Tracking issue already exists: ${url}`)
            }

            // Announce issue if issue does not already exist
            if (created) {
                await postMessage(
                    `:captain: Patch release ${release.version} will be published soon. If you have changes that should go into this patch release, please add your item to the checklist in the issue description: ${url}`,
                    slackAnnounceChannel
                )
                console.log(`Posted to Slack channel ${slackAnnounceChannel}`)
            }
        },
    },
    {
        id: 'changelog:cut',
        argNames: ['changelogFile'],
        run: async (config, changelogFile = 'CHANGELOG.md') => {
            const { upcoming: release } = await releaseVersions(config)

            await createChangesets({
                requiredCommands: [],
                changes: [
                    {
                        owner: 'sourcegraph',
                        repo: 'sourcegraph',
                        base: 'main',
                        head: `publish-${release.version}`,
                        commitMessage: `release: sourcegraph@${release.version}`,
                        edits: [
                            (directory: string) => {
                                console.log(`Updating '${changelogFile} for ${release.format()}'`)
                                const changelogPath = path.join(directory, changelogFile)
                                let changelogContents = readFileSync(changelogPath).toString()

                                // Convert 'unreleased' to a release
                                const releaseHeader = `## ${release.format()}`
                                const unreleasedHeader = '## Unreleased'
                                changelogContents = changelogContents.replace(unreleasedHeader, releaseHeader)

                                // Add a blank changelog template for the next release
                                changelogContents = changelogContents.replace(
                                    changelog.divider,
                                    changelog.releaseTemplate
                                )

                                // Update changelog
                                writeFileSync(changelogPath, changelogContents)
                            },
                        ], // Changes already done
                        title: `changelog: cut sourcegraph@${release.version}`,
                    },
                ],
                dryRun: config.dryRun.changesets,
            })
        },
    },
    {
        id: 'release:status',
        run: async config => {
            const githubClient = await getAuthenticatedGitHubClient()
            const { upcoming: release } = await releaseVersions(config)

            const trackingIssueURL = await getIssueByTitle(
                githubClient,
                trackingIssueTitle(release.major, release.minor)
            )
            if (!trackingIssueURL) {
                throw new Error(`Tracking issue for version ${release.version} not found - has it been created yet?`)
            }

            const blockingQuery = 'is:open org:sourcegraph label:release-blocker'
            const blockingIssues = await listIssues(githubClient, blockingQuery)
            const blockingIssuesURL = `https://github.com/issues?q=${encodeURIComponent(blockingQuery)}`

            const releaseMilestone = `${release.major}.${release.minor}${
                release.patch !== 0 ? `.${release.patch}` : ''
            }`
            const openQuery = `is:open org:sourcegraph is:issue milestone:${releaseMilestone}`
            const openIssues = await listIssues(githubClient, openQuery)
            const openIssuesURL = `https://github.com/issues?q=${encodeURIComponent(openQuery)}`

            const issueCategories = [
                { name: 'release-blocking', issues: blockingIssues, issuesURL: blockingIssuesURL },
                { name: 'open', issues: openIssues, issuesURL: openIssuesURL },
            ]

            const message = `:captain: ${release.version} release status update:

- Tracking issue: ${trackingIssueURL}
${issueCategories
    .map(
        category =>
            '- ' +
            (category.issues.length === 1
                ? `There is 1 ${category.name} issue: ${category.issuesURL}`
                : `There are ${category.issues.length} ${category.name} issues: ${category.issuesURL}`)
    )
    .join('\n')}`
            await postMessage(message, config.slackAnnounceChannel)
        },
    },
    {
        id: 'release:create-candidate',
        argNames: ['candidate'],
        run: async (config, candidate) => {
            if (!candidate) {
                throw new Error('Candidate information is required (either "final" or a number)')
            }
            const { upcoming: release } = await releaseVersions(config)

            const tag = JSON.stringify(`v${release.version}${candidate === 'final' ? '' : `-rc.${candidate}`}`)
            const branch = JSON.stringify(`${release.major}.${release.minor}`)

            console.log(`Creating and pushing tag ${tag} on ${branch}`)
            await execa(
                'bash',
                [
                    '-c',
                    `git diff --quiet && git checkout ${branch} && git pull --rebase && git tag -a ${tag} -m ${tag} && git push origin ${tag}`,
                ],
                { stdio: 'inherit' }
            )
        },
    },
    {
        id: 'release:stage',
        run: async config => {
            const { slackAnnounceChannel, dryRun } = config
            const { upcoming: release } = await releaseVersions(config)

            // set up src-cli
            await commandExists('src')
            const sourcegraphAuth = await campaigns.sourcegraphAuth()

            // Render changes
            const createdChanges = await createChangesets({
                requiredCommands: ['comby', sed, 'find', 'go'],
                changes: [
                    {
                        owner: 'sourcegraph',
                        repo: 'sourcegraph',
                        base: 'main',
                        head: `publish-${release.version}`,
                        commitMessage: `release: sourcegraph@${release.version}`,
                        title: `release: sourcegraph@${release.version}`,
                        edits: [
                            `find . -type f -name '*.md' ! -name 'CHANGELOG.md' -exec ${sed} -i -E 's/sourcegraph\\/server:[0-9]+\\.[0-9]+\\.[0-9]+/sourcegraph\\/server:${release.version}/g' {} +`,
                            `${sed} -i -E 's/version \`[0-9]+\\.[0-9]+\\.[0-9]+\`/version \`${release.version}\`/g' doc/index.md`,
                            release.patch === 0
                                ? `comby -in-place '{{$previousReleaseRevspec := ":[1]"}} {{$previousReleaseVersion := ":[2]"}} {{$currentReleaseRevspec := ":[3]"}} {{$currentReleaseVersion := ":[4]"}}' '{{$previousReleaseRevspec := ":[3]"}} {{$previousReleaseVersion := ":[4]"}} {{$currentReleaseRevspec := "v${release.version}"}} {{$currentReleaseVersion := "${release.major}.${release.minor}"}}' doc/_resources/templates/document.html`
                                : `comby -in-place 'currentReleaseRevspec := ":[1]"' 'currentReleaseRevspec := "v${release.version}"' doc/_resources/templates/document.html`,
                            `comby -in-place 'latestReleaseKubernetesBuild = newBuild(":[1]")' "latestReleaseKubernetesBuild = newBuild(\\"${release.version}\\")" cmd/frontend/internal/app/updatecheck/handler.go`,
                            `comby -in-place 'latestReleaseDockerServerImageBuild = newBuild(":[1]")' "latestReleaseDockerServerImageBuild = newBuild(\\"${release.version}\\")" cmd/frontend/internal/app/updatecheck/handler.go`,
                        ],
                    },
                    {
                        owner: 'sourcegraph',
                        repo: 'deploy-sourcegraph',
                        base: `${release.major}.${release.minor}`,
                        head: `publish-${release.version}`,
                        commitMessage: `release: sourcegraph@${release.version}`,
                        title: `release: sourcegraph@${release.version}`,
                        edits: [
                            // installs version pinned by deploy-sourcegraph
                            'go install github.com/slimsag/update-docker-tags',
                            `.github/workflows/scripts/update-docker-tags.sh ${release.version}`,
                        ],
                    },
                    {
                        owner: 'sourcegraph',
                        repo: 'deploy-sourcegraph-aws',
                        base: 'master',
                        head: `publish-${release.version}`,
                        commitMessage: `release: sourcegraph@${release.version}`,
                        title: `release: sourcegraph@${release.version}`,
                        edits: [
                            `${sed} -i -E 's/export SOURCEGRAPH_VERSION=[0-9]+\\.[0-9]+\\.[0-9]+/export SOURCEGRAPH_VERSION=${release.version}/g' resources/amazon-linux2.sh`,
                        ],
                    },
                    {
                        owner: 'sourcegraph',
                        repo: 'deploy-sourcegraph-digitalocean',
                        base: 'master',
                        head: `publish-${release.version}`,
                        commitMessage: `release: sourcegraph@${release.version}`,
                        title: `release: sourcegraph@${release.version}`,
                        edits: [
                            `${sed} -i -E 's/export SOURCEGRAPH_VERSION=[0-9]+\\.[0-9]+\\.[0-9]+/export SOURCEGRAPH_VERSION=${release.version}/g' resources/user-data.sh`,
                        ],
                    },
                ],
                dryRun: dryRun.changesets,
            })

            if (!dryRun.changesets) {
                // Create campaign to track changes
                let publishCampaign = ''
                try {
                    console.log(`Creating campaign in ${sourcegraphAuth.SRC_ENDPOINT}`)
                    publishCampaign = await campaigns.createCampaign(
                        createdChanges,
                        campaigns.releaseTrackingCampaign(release.version, sourcegraphAuth)
                    )
                    console.log(`Created ${publishCampaign}`)
                } catch (error) {
                    console.error(error)
                    console.error('Failed to create campaign for this release, omitting')
                }

                // Announce release update in Slack
                await postMessage(
                    `:captain: *Sourcegraph ${release.version} release has been staged*

* Campaign: ${publishCampaign}
* @stephen: update <https://github.com/sourcegraph/deploy-sourcegraph-docker|deploy-sourcegraph-docker> as needed`,
                    slackAnnounceChannel
                )
            }
        },
    },
    {
        id: 'release:add-to-campaign',
        // Example: yarn run release release:add-to-campaign sourcegraph/about 1797
        argNames: ['changeRepo', 'changeID'],
        run: async (config, changeRepo, changeID) => {
            const { upcoming: release } = await releaseVersions(config)
            if (!changeRepo || !changeID) {
                throw new Error('Missing parameters (required: version, repo, change ID)')
            }

            // set up src-cli
            await commandExists('src')
            const sourcegraphAuth = await campaigns.sourcegraphAuth()

            const campaignURL = await campaigns.addToCampaign(
                [
                    {
                        repository: changeRepo,
                        pullRequestNumber: parseInt(changeID, 10),
                    },
                ],
                campaigns.releaseTrackingCampaign(release.version, sourcegraphAuth)
            )
            console.log(`Added ${changeRepo}#${changeID} to campaign ${campaignURL}`)
        },
    },
    {
        id: 'release:close',
        run: async config => {
            const { slackAnnounceChannel } = config
            const { upcoming: release } = await releaseVersions(config)

            const versionAnchor = release.version.replace('.', '-')
            const campaignURL = campaigns.campaignURL(
                campaigns.releaseTrackingCampaign(release.version, await campaigns.sourcegraphAuth())
            )
            await postMessage(
                `:captain: *${release.version} has been published*

* Changelog: https://sourcegraph.com/github.com/sourcegraph/sourcegraph/-/blob/CHANGELOG.md#${versionAnchor}
* Release campaign: ${campaignURL}`,
                slackAnnounceChannel
            )
            console.log(`Posted to Slack channel ${slackAnnounceChannel}`)
        },
    },
    {
        id: '_test:google-calendar',
        run: async config => {
            const googleCalendar = await getClient()
            await ensureEvent(
                {
                    title: 'TEST EVENT',
                    startDateTime: new Date(config.releaseDateTime).toISOString(),
                    endDateTime: addMinutes(new Date(config.releaseDateTime), 1).toISOString(),
                },
                googleCalendar
            )
        },
    },
    {
        id: '_test:slack',
        run: async (_config, message) => {
            await postMessage(message, '_test-channel')
        },
    },
    {
        // Example: yarn run release _test:campaign-create-from-changes "$(cat ./.secrets/import.json)"
        id: '_test:campaign-create-from-changes',
        run: async (_config, campaignConfigJSON) => {
            const campaignConfig = JSON.parse(campaignConfigJSON) as {
                changes: CreatedChangeset[]
                name: string
                description: string
            }

            // set up src-cli
            await commandExists('src')
            const sourcegraphAuth = await campaigns.sourcegraphAuth()

            const campaignURL = await campaigns.createCampaign(campaignConfig.changes, {
                name: campaignConfig.name,
                description: campaignConfig.description,
                namespace: 'sourcegraph',
                auth: sourcegraphAuth,
            })
            console.log(`Created campaign ${campaignURL}`)
        },
    },
]

async function run(config: Config, stepIDToRun: StepID, ...stepArguments: string[]): Promise<void> {
    await Promise.all(
        steps
            .filter(({ id }) => id === stepIDToRun)
            .map(async step => {
                if (step.run) {
                    await step.run(config, ...stepArguments)
                }
            })
    )
}

/**
 * Release captain automation
 */
async function main(): Promise<void> {
    const config = persistedConfig
    const args = process.argv.slice(2)
    if (args.length === 0) {
        console.error('This command expects at least 1 argument')
        await run(config, 'help')
        return
    }
    const step = args[0]
    if (!steps.map(({ id }) => id as string).includes(step)) {
        console.error('Unrecognized step', JSON.stringify(step))
        return
    }
    const stepArguments = args.slice(1)
    await run(config, step as StepID, ...stepArguments)
}

main().catch(error => console.error(error))
