import { FC } from 'react'

import { mdiArrowExpand } from '@mdi/js'

import { SearchPatternType } from '@sourcegraph/shared/src/schema'
import { Button, Icon } from '@sourcegraph/wildcard'

import {
    AggregationChartCard,
    AggregationModeControls,
    AggregationLimitLabel,
    AggregationUIMode,
    useAggregationSearchMode,
    useAggregationUIMode,
    useSearchAggregationData,
    isNonExhaustiveAggregationResults,
} from '../components/aggregation'

import styles from './SearchAggregations.module.scss'

interface SearchAggregationsProps {
    /**
     * Current submitted query, note that this query isn't a live query
     * that is synced with typed query in the search box, this query is submitted
     * see `searchQueryFromURL` state in the global query Zustand store.
     */
    query: string

    /** Current search query pattern type. */
    patternType: SearchPatternType

    /** Whether to proactively load and display search aggregations */
    proactive: boolean

    /**
     * Emits whenever a user clicks one of aggregation chart segments (bars).
     * That should update the query and re-trigger search (but this should be connected
     * to this UI through its consumer)
     */
    onQuerySubmit: (newQuery: string) => void
}

export const SearchAggregations: FC<SearchAggregationsProps> = props => {
    const { query, patternType, proactive, onQuerySubmit } = props

    const [, setAggregationUIMode] = useAggregationUIMode()
    const [aggregationMode, setAggregationMode] = useAggregationSearchMode()
    const { data, error, loading } = useSearchAggregationData({
        query,
        patternType,
        aggregationMode,
        proactive,
        limit: 10,
    })

    return (
        <article className="pt-2">
            <AggregationModeControls
                loading={loading}
                mode={aggregationMode}
                availability={data?.searchQueryAggregate?.modeAvailability}
                size="sm"
                onModeChange={setAggregationMode}
            />

            {(proactive || aggregationMode !== null) && (
                <>
                    <AggregationChartCard
                        aria-label="Sidebar search aggregation chart"
                        data={data?.searchQueryAggregate?.aggregations}
                        loading={loading}
                        error={error}
                        mode={aggregationMode}
                        className={styles.chartContainer}
                        onBarLinkClick={onQuerySubmit}
                    />

                    <footer className={styles.actions}>
                        <Button
                            variant="secondary"
                            size="sm"
                            outline={true}
                            className={styles.detailsAction}
                            data-testid="expand-aggregation-ui"
                            onClick={() => setAggregationUIMode(AggregationUIMode.SearchPage)}
                        >
                            <Icon aria-hidden={true} svgPath={mdiArrowExpand} /> Expand
                        </Button>

                        {isNonExhaustiveAggregationResults(data) && <AggregationLimitLabel size="sm" />}
                    </footer>
                </>
            )}
        </article>
    )
}
