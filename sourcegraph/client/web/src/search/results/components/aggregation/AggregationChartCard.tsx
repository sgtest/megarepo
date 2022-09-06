import { Suspense, HTMLAttributes, ReactElement, MouseEvent, FC, SVGProps, forwardRef } from 'react'

import classNames from 'classnames'

import { ErrorAlert, ErrorMessage } from '@sourcegraph/branded/src/components/alerts'
import { SearchAggregationMode } from '@sourcegraph/shared/src/graphql-operations'
import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'
import { Text, Link, Tooltip, ForwardReferenceComponent } from '@sourcegraph/wildcard'

import { SearchAggregationDatum, GetSearchAggregationResult } from '../../../../graphql-operations'

import type { AggregationChartProps } from './AggregationChart'

import styles from './AggregationChartCard.module.scss'

const LazyAggregationChart = lazyComponent<AggregationChartProps<SearchAggregationDatum>, string>(
    () => import('./AggregationChart'),
    'AggregationChart'
)

/** Set custom value for minimal rotation angle for X ticks in sidebar UI panel mode. */
const MIN_X_TICK_ROTATION = 30
const MAX_SHORT_LABEL_WIDTH = 8
const MAX_LABEL_WIDTH = 16

const getName = (datum: SearchAggregationDatum): string => datum.label ?? ''
const getValue = (datum: SearchAggregationDatum): number => datum.count
const getLink = (datum: SearchAggregationDatum): string => datum.query ?? ''
const getColor = (): string => 'var(--primary)'

/**
 * Nested aggregation results types from {@link AGGREGATION_SEARCH_QUERY} GQL query
 */
type SearchAggregationResult = GetSearchAggregationResult['searchQueryAggregate']['aggregations']

function getAggregationError(aggregation?: SearchAggregationResult): Error | undefined {
    if (aggregation?.__typename === 'SearchAggregationNotAvailable') {
        return new Error(aggregation.reason)
    }

    return
}

export function getAggregationData(aggregations: SearchAggregationResult): SearchAggregationDatum[] {
    switch (aggregations?.__typename) {
        case 'ExhaustiveSearchAggregationResult':
        case 'NonExhaustiveSearchAggregationResult':
            return aggregations.groups

        default:
            return []
    }
}

export function getOtherGroupCount(aggregations: SearchAggregationResult): number {
    switch (aggregations?.__typename) {
        case 'ExhaustiveSearchAggregationResult':
            return aggregations.otherGroupCount ?? 0
        case 'NonExhaustiveSearchAggregationResult':
            return aggregations.approximateOtherGroupCount ?? 0

        default:
            return 0
    }
}

interface AggregationChartCardProps extends HTMLAttributes<HTMLDivElement> {
    data?: SearchAggregationResult
    error?: Error
    loading: boolean
    mode?: SearchAggregationMode | null
    size?: 'sm' | 'md'
    onBarLinkClick?: (query: string, barIndex: number) => void
    onBarHover?: () => void
}

export function AggregationChartCard(props: AggregationChartCardProps): ReactElement | null {
    const {
        data,
        error,
        loading,
        mode,
        className,
        size = 'sm',
        'aria-label': ariaLabel,
        onBarLinkClick,
        onBarHover,
    } = props

    if (loading) {
        return (
            <DataLayoutContainer size={size} className={classNames(styles.loading, className)}>
                Loading...
            </DataLayoutContainer>
        )
    }

    // Internal error
    if (error) {
        return (
            <DataLayoutContainer size={size} className={className}>
                <ErrorAlert error={error} className={styles.errorAlert} />
            </DataLayoutContainer>
        )
    }

    const aggregationError = getAggregationError(data)

    if (aggregationError) {
        return (
            <DataLayoutContainer
                data-error-layout={true}
                size={size}
                className={classNames(styles.aggregationErrorContainer, className)}
            >
                <BarsBackground size={size} />
                <div className={styles.errorMessageLayout}>
                    <div className={styles.errorMessage}>
                        We couldn’t provide an aggregation for this query. <ErrorMessage error={aggregationError} />{' '}
                        <Link to="">Learn more</Link>
                    </div>
                </div>
            </DataLayoutContainer>
        )
    }

    if (!data) {
        return null
    }

    const missingCount = getOtherGroupCount(data)
    const handleDatumLinkClick = (event: MouseEvent, datum: SearchAggregationDatum, index: number): void => {
        event.preventDefault()
        onBarLinkClick?.(getLink(datum), index)
    }

    return (
        <div className={classNames(className, styles.container)}>
            <Suspense>
                <LazyAggregationChart
                    aria-label={ariaLabel}
                    data={getAggregationData(data)}
                    mode={mode}
                    minAngleXTick={size === 'md' ? 0 : MIN_X_TICK_ROTATION}
                    maxXLabelLength={size === 'md' ? MAX_LABEL_WIDTH : MAX_SHORT_LABEL_WIDTH}
                    getDatumValue={getValue}
                    getDatumColor={getColor}
                    getDatumName={getName}
                    getDatumLink={getLink}
                    onDatumLinkClick={handleDatumLinkClick}
                    onDatumHover={onBarHover}
                    className={styles.chart}
                />

                {!!missingCount && (
                    <Tooltip
                        content={`There are ${missingCount} more groups that were not included in this aggregation.`}
                    >
                        <Text size="small" className={styles.missingLabelCount}>
                            +{missingCount}
                        </Text>
                    </Tooltip>
                )}
            </Suspense>
        </div>
    )
}

interface DataLayoutContainerProps {
    size?: 'sm' | 'md'
}

const DataLayoutContainer = forwardRef((props, ref) => {
    const { as: Component = 'div', size = 'md', className, ...attributes } = props

    return (
        <Component
            {...attributes}
            ref={ref}
            className={classNames(className, styles.errorContainer, {
                [styles.errorContainerSmall]: size === 'sm',
            })}
        />
    )
}) as ForwardReferenceComponent<'div', DataLayoutContainerProps>

const BAR_VALUES_FULL_UI = [95, 88, 83, 70, 65, 45, 35, 30, 30, 30, 30, 27, 27, 27, 27, 24, 10, 10, 10, 10, 10]
const BAR_VALUES_SIDEBAR_UI = [95, 80, 75, 70, 68, 68, 55, 40, 38, 33, 30, 25, 15, 7]

interface BarsBackgroundProps extends SVGProps<SVGSVGElement> {
    size: 'sm' | 'md'
}

const BarsBackground: FC<BarsBackgroundProps> = props => {
    const { size, className, ...attributes } = props

    const padding = size === 'md' ? 1 : 2
    const data = size === 'md' ? BAR_VALUES_FULL_UI : BAR_VALUES_SIDEBAR_UI
    const barWidth = (100 - padding * (data.length - 1)) / data.length

    return (
        <svg
            {...attributes}
            className={classNames(className, styles.zeroStateBackground)}
            xmlns="http://www.w3.org/2000/svg"
        >
            {data.map((bar, index) => (
                <rect
                    key={index}
                    x={`${index * (barWidth + padding)}%`}
                    y={`${100 - bar}%`}
                    height={`${bar}%`}
                    width={`${barWidth}%`}
                />
            ))}
        </svg>
    )
}
