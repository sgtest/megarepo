import { forwardRef, HTMLAttributes, useContext, useRef, useState } from 'react'

import classNames from 'classnames'
import { useMergeRefs } from 'use-callback-ref'

import { isDefined } from '@sourcegraph/common'
import { useQuery } from '@sourcegraph/http-client'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Link, useDebounce, useDeepMemo, Text } from '@sourcegraph/wildcard'

import { GetInsightViewResult, GetInsightViewVariables } from '../../../../../../graphql-operations'
import { useSeriesToggle } from '../../../../../../insights/utils/use-series-toggle'
import {
    BackendInsight,
    BackendInsightData,
    CodeInsightsBackendContext,
    InsightFilters,
    isComputeInsight,
    useSaveInsightAsNewView,
} from '../../../../core'
import { GET_INSIGHT_VIEW_GQL } from '../../../../core/backend/gql-backend'
import { createBackendInsightData } from '../../../../core/backend/gql-backend/methods/get-backend-insight-data/deserializators'
import { insightPollingInterval } from '../../../../core/backend/gql-backend/utils/insight-polling'
import { getTrackingTypeByInsightType, useCodeInsightViewPings } from '../../../../pings'
import { InsightCard, InsightCardBanner, InsightCardHeader, InsightCardLoading } from '../../../views'
import { useVisibility } from '../../hooks/use-insight-data'
import { InsightContextMenu } from '../insight-context-menu/InsightContextMenu'
import { InsightContext } from '../InsightContext'

import {
    BackendInsightErrorAlert,
    DrillDownFiltersPopover,
    DrillDownInsightCreationFormValues,
    BackendInsightChart,
    InsightIncompleteAlert,
} from './components'

import styles from './BackendInsight.module.scss'

interface BackendInsightProps extends TelemetryProps, HTMLAttributes<HTMLElement> {
    insight: BackendInsight
    resizing?: boolean
}

export const BackendInsightView = forwardRef<HTMLElement, BackendInsightProps>((props, ref) => {
    const { telemetryService, insight, resizing, children, className, ...attributes } = props

    const { currentDashboard } = useContext(InsightContext)
    const { updateInsight } = useContext(CodeInsightsBackendContext)
    const [saveNewView] = useSaveInsightAsNewView({ dashboard: currentDashboard })

    const cardElementRef = useMergeRefs([ref])
    const { wasEverVisible, isVisible } = useVisibility(cardElementRef)

    const seriesToggleState = useSeriesToggle()

    // Original insight filters values that are stored in setting subject with insight
    // configuration object, They are updated  whenever the user clicks update/save button
    const [originalInsightFilters, setOriginalInsightFilters] = useState(insight.filters)

    // Live valid filters from filter form. They are updated whenever the user is changing
    // filter value in filters fields.
    const [filters, setFilters] = useState<InsightFilters>(originalInsightFilters)
    const [insightData, setInsightData] = useState<BackendInsightData | undefined>()
    const [zeroYAxisMin, setZeroYAxisMin] = useState(false)
    const [isFiltersOpen, setIsFiltersOpen] = useState(false)

    const debouncedFilters = useDebounce(useDeepMemo<InsightFilters>(filters), 500)

    const { error, loading, stopPolling, startPolling } = useQuery<GetInsightViewResult, GetInsightViewVariables>(
        GET_INSIGHT_VIEW_GQL,
        {
            skip: !wasEverVisible,
            context: { concurrentRequests: { key: 'GET_INSIGHT_VIEW' } },
            variables: {
                id: insight.id,
                filters: {
                    includeRepoRegex: debouncedFilters.includeRepoRegexp,
                    excludeRepoRegex: debouncedFilters.excludeRepoRegexp,
                    searchContexts: [debouncedFilters.context],
                },
                seriesDisplayOptions: {
                    numSamples: debouncedFilters.seriesDisplayOptions.numSamples,
                    limit: debouncedFilters.seriesDisplayOptions.limit,
                    sortOptions: debouncedFilters.seriesDisplayOptions.sortOptions,
                },
            },
            onCompleted: data => {
                // This query requests a list of 1 insight view if there is an error and the insightView
                // will be null and error is populated
                const node = data.insightViews.nodes[0]

                seriesToggleState.setSelectedSeriesIds([])
                setInsightData(isDefined(node) ? createBackendInsightData({ ...insight, filters }, node) : undefined)
            },
        }
    )

    const isFetchingHistoricalData = insightData?.isFetchingHistoricalData
    const isPolling = useRef(false)

    // Not on the screen so stop polling if we are - multiple stop calls are safe
    if (error || !isVisible || !isFetchingHistoricalData) {
        isPolling.current = false
        stopPolling()
    } else if (isFetchingHistoricalData && !isPolling.current) {
        // we should start polling but multiple calls to startPolling reset the timer so
        // make sure we aren't already polling.
        isPolling.current = true
        startPolling(insightPollingInterval(insight))
    }

    async function handleFilterSave(filters: InsightFilters): Promise<void> {
        const insightWithNewFilters = { ...insight, filters }

        await updateInsight({ insightId: insight.id, nextInsightData: insightWithNewFilters }).toPromise()

        telemetryService.log('CodeInsightsSearchBasedFilterUpdating')
        setOriginalInsightFilters(filters)
        setIsFiltersOpen(false)
    }

    const handleInsightFilterCreation = async (values: DrillDownInsightCreationFormValues): Promise<void> => {
        const { insightName } = values

        await saveNewView({
            insight,
            filters,
            title: insightName,
            dashboard: currentDashboard,
        })

        telemetryService.log('CodeInsightsSearchBasedFilterInsightCreation')
        setOriginalInsightFilters(filters)
        setIsFiltersOpen(false)
    }

    const { trackMouseLeave, trackMouseEnter, trackDatumClicks } = useCodeInsightViewPings({
        telemetryService,
        insightType: getTrackingTypeByInsightType(insight.type),
    })

    return (
        <InsightCard
            {...attributes}
            ref={cardElementRef}
            data-testid={`insight-card.${insight.id}`}
            aria-label={`${insight.title} insight`}
            role="listitem"
            className={classNames(className, { [styles.cardWithFilters]: isFiltersOpen })}
            onMouseEnter={trackMouseEnter}
            onMouseLeave={trackMouseLeave}
        >
            <InsightCardHeader
                title={
                    <Link
                        to={`${window.location.origin}/insights/${insight.id}`}
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        {insight.title}
                    </Link>
                }
                subtitle={
                    isFetchingHistoricalData && (
                        <Text size="small" className="text-muted">
                            Datapoints shown may be undercounted.{' '}
                            <Link
                                to="/help/code_insights/explanations/current_limitations_of_code_insights#performance-speed-considerations-for-a-data-series-running-over-all-repositories"
                                target="_blank"
                                rel="noopener noreferrer"
                            >
                                Processing
                            </Link>{' '}
                            time may vary depending on the insight’s scope.
                        </Text>
                    )
                }
            >
                {isVisible && (
                    <>
                        {insightData?.incompleteAlert && <InsightIncompleteAlert alert={insightData.incompleteAlert} />}
                        <DrillDownFiltersPopover
                            isOpen={isFiltersOpen}
                            anchor={cardElementRef}
                            initialFiltersValue={filters}
                            originalFiltersValue={originalInsightFilters}
                            // It doesn't make sense to have max series per point for compute insights
                            // because there is always only one point per series
                            isNumSamplesFilterAvailable={!isComputeInsight(insight)}
                            onFilterChange={setFilters}
                            onFilterSave={handleFilterSave}
                            onInsightCreate={handleInsightFilterCreation}
                            onVisibilityChange={setIsFiltersOpen}
                        />
                        <InsightContextMenu
                            insight={insight}
                            currentDashboard={currentDashboard}
                            zeroYAxisMin={zeroYAxisMin}
                            onToggleZeroYAxisMin={() => setZeroYAxisMin(!zeroYAxisMin)}
                        />
                    </>
                )}
            </InsightCardHeader>

            {resizing ? (
                <InsightCardBanner>Resizing</InsightCardBanner>
            ) : error ? (
                <BackendInsightErrorAlert error={error} />
            ) : loading || !isVisible || !insightData ? (
                <InsightCardLoading>Loading code insight</InsightCardLoading>
            ) : (
                <BackendInsightChart
                    {...insightData}
                    locked={insight.isFrozen}
                    zeroYAxisMin={zeroYAxisMin}
                    seriesToggleState={seriesToggleState}
                    onDatumClick={trackDatumClicks}
                />
            )}
            {
                // Passing children props explicitly to render any top-level content like
                // resize-handler from the react-grid-layout library
                isVisible && children
            }
        </InsightCard>
    )
})
