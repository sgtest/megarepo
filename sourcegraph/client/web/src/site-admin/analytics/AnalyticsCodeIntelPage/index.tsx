import React, { useMemo, useEffect } from 'react'

import classNames from 'classnames'
import { RouteComponentProps } from 'react-router'

import { useQuery } from '@sourcegraph/http-client'
import { Card, H2, Text, LoadingSpinner, AnchorLink, H4 } from '@sourcegraph/wildcard'

import { LineChart, Series } from '../../../charts'
import { CodeIntelStatisticsResult, CodeIntelStatisticsVariables } from '../../../graphql-operations'
import { eventLogger } from '../../../tracking/eventLogger'
import { AnalyticsPageTitle } from '../components/AnalyticsPageTitle'
import { ChartContainer } from '../components/ChartContainer'
import { HorizontalSelect } from '../components/HorizontalSelect'
import { TimeSavedCalculatorGroup } from '../components/TimeSavedCalculatorGroup'
import { ToggleSelect } from '../components/ToggleSelect'
import { ValueLegendList, ValueLegendListProps } from '../components/ValueLegendList'
import { useChartFilters } from '../useChartFilters'
import { StandardDatum } from '../utils'

import { CODEINTEL_STATISTICS } from './queries'

import styles from './index.module.scss'

export const AnalyticsCodeIntelPage: React.FunctionComponent<RouteComponentProps<{}>> = () => {
    const { dateRange, aggregation, grouping } = useChartFilters({ name: 'CodeIntel' })
    const { data, error, loading } = useQuery<CodeIntelStatisticsResult, CodeIntelStatisticsVariables>(
        CODEINTEL_STATISTICS,
        {
            variables: {
                dateRange: dateRange.value,
                grouping: grouping.value,
            },
        }
    )
    useEffect(() => {
        eventLogger.logPageView('AdminAnalyticsCodeIntel')
    }, [])
    const [stats, legends, calculatorProps] = useMemo(() => {
        if (!data) {
            return []
        }
        const {
            referenceClicks,
            definitionClicks,
            inAppEvents,
            codeHostEvents,
            searchBasedEvents,
            preciseEvents,
            crossRepoEvents,
        } = data.site.analytics.codeIntel

        const totalEvents = definitionClicks.summary.totalCount + referenceClicks.summary.totalCount
        const totalHoverEvents = searchBasedEvents.summary.totalCount + preciseEvents.summary.totalCount

        const stats: Series<StandardDatum>[] = [
            {
                id: 'references',
                name:
                    aggregation.selected === 'count'
                        ? '"Find references" clicked'
                        : 'Users who clicked "Find references"',
                color: 'var(--cyan)',
                data: referenceClicks.nodes.map(
                    node => ({
                        date: new Date(node.date),
                        value: node[aggregation.selected],
                    }),
                    dateRange.value
                ),
                getXValue: ({ date }) => date,
                getYValue: ({ value }) => value,
            },
            {
                id: 'definitions',
                name:
                    aggregation.selected === 'count'
                        ? '"Go to definition" clicked'
                        : 'Users who clicked "Go to definition"',
                color: 'var(--orange)',
                data: definitionClicks.nodes.map(
                    node => ({
                        date: new Date(node.date),
                        value: node[aggregation.selected],
                    }),
                    dateRange.value
                ),
                getXValue: ({ date }) => date,
                getYValue: ({ value }) => value,
            },
        ]
        const legends: ValueLegendListProps['items'] = [
            {
                value: referenceClicks.summary[aggregation.selected === 'count' ? 'totalCount' : 'totalUniqueUsers'],
                description: aggregation.selected === 'count' ? 'References' : 'Users using references',
                color: 'var(--cyan)',
            },
            {
                value: definitionClicks.summary[aggregation.selected === 'count' ? 'totalCount' : 'totalUniqueUsers'],
                description: aggregation.selected === 'count' ? 'Definitions' : 'Users using definitions',
                color: 'var(--orange)',
            },
            {
                value: Math.floor((crossRepoEvents.summary.totalCount * totalEvents) / totalHoverEvents || 0),
                description: 'Cross repo events',
                position: 'right',
                color: 'var(--body-color)',
                tooltip:
                    'Cross repository code intel identifies symbols in code throughout your Sourcegraph instance, in a single click, without locating and downloading a repository.',
            },
        ]

        const calculatorProps = {
            page: 'CodeIntel',
            label: 'Intel events',
            dateRange: dateRange.value,
            color: 'var(--purple)',
            description:
                'Code navigation helps users quickly understand a codebase, identify dependencies, reuse code, and perform more efficient and accurate code reviews.<br/><br/>We’ve broken this calculation down into use cases and types of code intel to be able to independently value product capabilities.',
            value: totalEvents,
            items: [
                {
                    label: 'In app code navigation',
                    minPerItem: 0.5,
                    value: inAppEvents.summary.totalCount,
                    description:
                        'In app code navigation supports developers finding the impact of a change or code to reuse by listing references and finding definitions.',
                },
                {
                    label: 'Code intel on code hosts <br/> via the browser extension',
                    minPerItem: 1.5,
                    value: codeHostEvents.summary.totalCount,
                    description:
                        'Intel events on the code host typically occur during PR reviews, where the ability to quickly understand code is key to efficient reviews.',
                },
                {
                    label: 'Cross repository <br/> code intel events',
                    minPerItem: 3,
                    value: Math.floor((crossRepoEvents.summary.totalCount * totalEvents) / totalHoverEvents || 0),
                    description:
                        'Cross repository code intel identifies the correct symbol in code throughout your entire code base in a single click, without locating and downloading a repository.',
                },
                {
                    label: 'Precise code intel*',
                    minPerItem: 1,
                    value: Math.floor((preciseEvents.summary.totalCount * totalEvents) / totalHoverEvents || 0),
                    description:
                        'Compiler-accurate code intel takes users to the correct result as defined by SCIP, and does so cross repository. The reduction in false positives produced by other search engines represents significant additional time savings.',
                },
            ],
        }

        return [stats, legends, calculatorProps]
    }, [data, dateRange.value, aggregation.selected])

    if (error) {
        throw error
    }

    if (loading) {
        return <LoadingSpinner />
    }

    const repos = data?.site.analytics.repos

    return (
        <>
            <AnalyticsPageTitle>Analytics / Code intel</AnalyticsPageTitle>

            <Card className="p-3 position-relative">
                <div className="d-flex justify-content-end align-items-stretch mb-2 text-nowrap">
                    <HorizontalSelect<typeof dateRange.value> {...dateRange} />
                </div>
                {legends && <ValueLegendList className="mb-3" items={legends} />}
                {stats && (
                    <div>
                        <ChartContainer
                            title={aggregation.selected === 'count' ? 'Activity by day' : 'Unique users by day'}
                            labelX="Time"
                            labelY={aggregation.selected === 'count' ? 'Activity' : 'Unique users'}
                        >
                            {width => <LineChart width={width} height={300} series={stats} />}
                        </ChartContainer>
                        <div className="d-flex justify-content-end align-items-stretch mb-2 text-nowrap">
                            <HorizontalSelect<typeof grouping.value> {...grouping} className="mr-4" />
                            <ToggleSelect<typeof aggregation.selected> {...aggregation} />
                        </div>
                    </div>
                )}
                <H2 className="my-3">Total time saved</H2>
                {calculatorProps && <TimeSavedCalculatorGroup {...calculatorProps} />}
                <div className={styles.suggestionBox}>
                    <H4 className="my-3">Suggestions</H4>
                    <div className={classNames(styles.border, 'mb-3')} />
                    <ul className="mb-3 pl-3">
                        <Text as="li">
                            Promote installation of the{' '}
                            <AnchorLink to="/help/integration/browser_extension" target="_blank">
                                browser extension
                            </AnchorLink>{' '}
                            to add code intelligence to your code hosts.
                        </Text>
                        {repos && (
                            <Text as="li">
                                <b>{repos.preciseCodeIntelCount}</b> of your <b>{repos.count}</b> repositories have
                                precise code intel.{' '}
                                <AnchorLink
                                    to="/help/code_intelligence/explanations/precise_code_intelligence"
                                    target="_blank"
                                >
                                    Learn how to improve precise code intel coverage.
                                </AnchorLink>
                            </Text>
                        )}
                    </ul>
                </div>
            </Card>
            <Text className="font-italic text-center mt-2">
                All events are generated from entries in the event logs table and are updated every 24 hours.
                <br />* Calculated from precise code intel events
            </Text>
        </>
    )
}
