import { ParentSize } from '@visx/responsive'
import classNames from 'classnames'
import React from 'react'
import { useLocation } from 'react-router'

import { SyntaxHighlightedSearchQuery } from '@sourcegraph/search-ui'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Button, Link } from '@sourcegraph/wildcard'

import * as View from '../../../../../../views'
import { LegendBlock, LegendItem } from '../../../../../../views'
import {
    getLineStroke,
    LineChart,
} from '../../../../../../views/components/view/content/chart-view-content/charts/line/components/LineChartContent'
import { InsightType } from '../../../../core/types'
import { CodeInsightTrackType, useCodeInsightViewPings } from '../../../../pings'
import { encodeCaptureInsightURL } from '../../../insights/creation/capture-group'
import { encodeSearchInsightUrl } from '../../../insights/creation/search-insight'
import { CodeInsightsQueryBlock } from '../code-insights-query-block/CodeInsightsQueryBlock'

import styles from './CodeInsightsExamples.module.scss'
import { ALPINE_VERSIONS_INSIGHT, CSS_MODULES_VS_GLOBAL_STYLES_INSIGHT } from './examples'
import { CaptureGroupExampleContent, SearchInsightExampleContent } from './types'

export interface CodeInsightsExamplesProps extends TelemetryProps, React.HTMLAttributes<HTMLElement> {}

const SEARCH_INSIGHT_CREATION_UI_URL_PARAMETERS = encodeSearchInsightUrl({
    title: CSS_MODULES_VS_GLOBAL_STYLES_INSIGHT.title,
    series: CSS_MODULES_VS_GLOBAL_STYLES_INSIGHT.series,
})

const CAPTURE_GROUP_INSIGHT_CREATION_UI_URL_PARAMETERS = encodeCaptureInsightURL({
    title: ALPINE_VERSIONS_INSIGHT.title,
    allRepos: true,
    groupSearchQuery: ALPINE_VERSIONS_INSIGHT.groupSearch,
})

export const CodeInsightsExamples: React.FunctionComponent<CodeInsightsExamplesProps> = props => {
    const { telemetryService, ...otherProps } = props
    const { pathname, search } = useLocation()

    return (
        <section {...otherProps}>
            <h2>Example insights</h2>
            <p className="text-muted">
                Here are a few example insights to show you what the tool can do.{' '}
                <Link to={`${pathname}${search}#code-insights-templates`}>Explore more use cases.</Link>
            </p>

            <div className={styles.section}>
                <CodeInsightExample
                    type={InsightType.SearchBased}
                    content={CSS_MODULES_VS_GLOBAL_STYLES_INSIGHT}
                    templateLink={`/insights/create/search?${SEARCH_INSIGHT_CREATION_UI_URL_PARAMETERS}`}
                    telemetryService={telemetryService}
                    className={styles.card}
                />

                <CodeInsightExample
                    type={InsightType.CaptureGroup}
                    content={ALPINE_VERSIONS_INSIGHT}
                    templateLink={`/insights/create/capture-group?${CAPTURE_GROUP_INSIGHT_CREATION_UI_URL_PARAMETERS}`}
                    telemetryService={telemetryService}
                    className={styles.card}
                />
            </div>
        </section>
    )
}

interface CodeInsightExampleCommonProps {
    templateLink?: string
    className?: string
}

export type CodeInsightExampleProps = (CodeInsightSearchExampleProps | CodeInsightCaptureExampleProps) &
    CodeInsightExampleCommonProps

export const CodeInsightExample: React.FunctionComponent<CodeInsightExampleProps> = props => {
    if (props.type === InsightType.SearchBased) {
        return <CodeInsightSearchExample {...props} />
    }

    return <CodeInsightCaptureExample {...props} />
}

interface CodeInsightSearchExampleProps extends TelemetryProps {
    type: InsightType.SearchBased
    content: SearchInsightExampleContent
    templateLink?: string
    className?: string
}

const CodeInsightSearchExample: React.FunctionComponent<CodeInsightSearchExampleProps> = props => {
    const { templateLink, className, content, telemetryService } = props
    const { trackMouseEnter, trackMouseLeave } = useCodeInsightViewPings({
        telemetryService,
        insightType: CodeInsightTrackType.InProductLandingPageInsight,
    })

    const handleTemplateLinkClick = (): void => {
        telemetryService.log('InsightsGetStartedBigTemplateClick')
    }

    return (
        <View.Root
            title={content.title}
            subtitle={
                <CodeInsightsQueryBlock
                    as={SyntaxHighlightedSearchQuery}
                    query={content.repositories}
                    className="mt-1"
                />
            }
            className={className}
            actions={
                templateLink && (
                    <Button
                        as={Link}
                        variant="link"
                        size="sm"
                        className={styles.actionLink}
                        to={templateLink}
                        onClick={handleTemplateLinkClick}
                    >
                        Use as template
                    </Button>
                )
            }
            onMouseEnter={trackMouseEnter}
            onMouseLeave={trackMouseLeave}
        >
            <div className={styles.chart}>
                <ParentSize>
                    {({ width, height }) => <LineChart {...content} width={width} height={height} />}
                </ParentSize>
            </div>

            <LegendBlock className={styles.legend}>
                {content.series.map(line => (
                    <LegendItem key={line.dataKey.toString()} color={getLineStroke<any>(line)}>
                        <span className={classNames(styles.legendItem, 'flex-shrink-0 mr-2')}>{line.name}</span>
                        <CodeInsightsQueryBlock as={SyntaxHighlightedSearchQuery} query={line.query} />
                    </LegendItem>
                ))}
            </LegendBlock>
        </View.Root>
    )
}

interface CodeInsightCaptureExampleProps extends TelemetryProps {
    type: InsightType.CaptureGroup
    content: CaptureGroupExampleContent
    templateLink?: string
    className?: string
}

const CodeInsightCaptureExample: React.FunctionComponent<CodeInsightCaptureExampleProps> = props => {
    const { content, templateLink, className, telemetryService } = props
    const { trackMouseEnter, trackMouseLeave } = useCodeInsightViewPings({
        telemetryService,
        insightType: CodeInsightTrackType.InProductLandingPageInsight,
    })

    const handleTemplateLinkClick = (): void => {
        telemetryService.log('GetStartedBigTemplateClick')
    }

    return (
        <View.Root
            title={content.title}
            subtitle={
                <CodeInsightsQueryBlock
                    as={SyntaxHighlightedSearchQuery}
                    query={content.repositories}
                    className="mt-1"
                />
            }
            actions={
                templateLink && (
                    <Button
                        as={Link}
                        variant="link"
                        size="sm"
                        className={styles.actionLink}
                        to={templateLink}
                        onClick={handleTemplateLinkClick}
                    >
                        Use as template
                    </Button>
                )
            }
            className={className}
            onMouseEnter={trackMouseEnter}
            onMouseLeave={trackMouseLeave}
        >
            <div className={styles.captureGroup}>
                <div className={styles.chart}>
                    <ParentSize className={styles.chartContent}>
                        {({ width, height }) => <LineChart {...content} width={width} height={height} />}
                    </ParentSize>
                </div>

                <LegendBlock className={styles.legend}>
                    {content.series.map(line => (
                        <LegendItem key={line.dataKey.toString()} color={getLineStroke<any>(line)}>
                            {line.name}
                        </LegendItem>
                    ))}
                </LegendBlock>
            </div>
            <CodeInsightsQueryBlock as={SyntaxHighlightedSearchQuery} query={content.groupSearch} className="mt-2" />
        </View.Root>
    )
}
