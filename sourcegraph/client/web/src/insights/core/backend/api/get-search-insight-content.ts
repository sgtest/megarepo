import { formatISO, isAfter, startOfDay, sub, Duration } from 'date-fns'
import escapeRegExp from 'lodash/escapeRegExp'
import { defer } from 'rxjs'
import { retry } from 'rxjs/operators'
import type { LineChartContent } from 'sourcegraph'

import { fetchRawSearchInsightResults, fetchSearchInsightCommits } from '../requests/fetch-search-insight'
import { SearchInsightSettings } from '../types'

/**
 * This logic is a copy of fetch logic of search-based code insight extension.
 * See https://github.com/sourcegraph/sourcegraph-search-insights/blob/master/src/search-insights.ts
 * In order to have live preview for creation UI we had to copy this logic from
 * extension.
 *
 * */
export async function getSearchInsightContent(insight: SearchInsightSettings): Promise<LineChartContent<any, string>> {
    const step = insight.step || { days: 1 }
    const { repositories: repos } = insight
    const dates = getDaysToQuery(step)

    // Get commits to search for each day
    const repoCommits = (
        await Promise.all(
            repos.map(async repo => (await determineCommitsToSearch(dates, repo)).map(commit => ({ repo, ...commit })))
        )
    ).flat()

    const searchQueries = insight.series.flatMap(({ query, name }) =>
        repoCommits.map(({ date, repo, commit }) => ({
            name,
            date,
            repo,
            commit,
            query: `repo:^${escapeRegExp(repo)}$@${commit} ${query} count:99999`,
        }))
    )
    const rawSearchResults = await defer(() => fetchRawSearchInsightResults(searchQueries.map(search => search.query)))
        // The bulk search may timeout, but a retry is then likely faster because caches are warm
        .pipe(retry(3))
        .toPromise()

    const searchResults = Object.entries(rawSearchResults).map(([field, result]) => {
        const index = +field.slice('search'.length)
        const query = searchQueries[index]
        return { ...query, result }
    })

    const data: {
        date: number
        [seriesName: string]: number
    }[] = []
    for (const { name, date, result } of searchResults) {
        const dataKey = name
        const dataIndex = dates.indexOf(date)
        const object =
            data[dataIndex] ??
            (data[dataIndex] = {
                date: date.getTime(),
                // Initialize all series to 0
                ...Object.fromEntries(insight.series.map(series => [series.name, 0])),
            })
        // Sum across repos
        const countForRepo = result?.results.matchCount

        object[dataKey] += countForRepo ?? 0
    }

    return {
        chart: 'line' as const,
        data,
        series: insight.series.map(series => ({
            dataKey: series.name,
            name: series.name,
            stroke: series.stroke,
            linkURLs: dates.map(date => {
                // Link to diff search that explains what new cases were added between two data points
                const url = new URL('/search', window.location.origin)
                // Use formatISO instead of toISOString(), because toISOString() always outputs UTC.
                // They mark the same point in time, but using the user's timezone makes the date string
                // easier to read (else the date component may be off by one day)
                const after = formatISO(sub(date, step))
                const before = formatISO(date)
                const repoFilters = repos.map(repo => `repo:^${escapeRegExp(repo)}$`).join(' ')
                const diffQuery = `${repoFilters} type:diff after:${after} before:${before} ${series.query}`
                url.searchParams.set('q', diffQuery)
                return url.href
            }),
        })),
        xAxis: {
            dataKey: 'date' as const,
            type: 'number' as const,
            scale: 'time' as const,
        },
    }
}

async function determineCommitsToSearch(dates: Date[], repo: string): Promise<{ date: Date; commit: string }[]> {
    const commitQueries = dates.map(date => {
        const before = formatISO(date)
        return `repo:^${escapeRegExp(repo)}$ type:commit before:${before} count:1`
    })

    const commitResults = await fetchSearchInsightCommits(commitQueries).toPromise()

    const commitOids = Object.entries(commitResults).map(([name, search], index) => {
        const index_ = +name.slice('search'.length)

        if (index_ !== index) {
            throw new Error(`Expected field ${name} to be at index ${index_} of object keys`)
        }

        if (search.results.results.length === 0) {
            throw new Error(`No result for ${commitQueries[index_]}`)
        }

        const commit = (search?.results.results[0]).commit

        // Sanity check
        const commitDate = commit.committer && new Date(commit.committer.date)
        const date = dates[index_]
        if (!commitDate) {
            throw new Error(`Expected commit to have committer: \`${commit.oid}\``)
        }
        if (isAfter(commitDate, date)) {
            throw new Error(
                `Expected commit \`${commit.oid}\` to be before ${formatISO(date)}, but was after: ${formatISO(
                    commitDate
                )}.\nSearch query: ${commitQueries[index_]}`
            )
        }

        return { commit: commit.oid, date }
    })

    return commitOids
}

const NUMBER_OF_CHART_POINTS = 7

function getDaysToQuery(step: Duration): Date[] {
    const now = startOfDay(new Date())
    const dates: Date[] = []

    for (let index = 0, date = now; index < NUMBER_OF_CHART_POINTS; index++) {
        dates.unshift(date)
        date = sub(date, step)
    }

    return dates
}
