import { getYear, parseISO } from 'date-fns'
import * as H from 'history'
import React, { useCallback, useMemo, useState } from 'react'
import {
    Area,
    ComposedChart,
    LabelFormatter,
    ResponsiveContainer,
    Tooltip,
    XAxis,
    YAxis,
    TooltipPayload,
} from 'recharts'

import { Toggle } from '@sourcegraph/branded/src/components/Toggle'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'

import { ChangesetCountsOverTimeFields, Scalars } from '../../../graphql-operations'

import { queryChangesetCountsOverTime as _queryChangesetCountsOverTime } from './backend'

interface Props {
    batchChangeID: Scalars['ID']
    history: H.History
    width?: string | number

    /** For testing only. */
    queryChangesetCountsOverTime?: typeof _queryChangesetCountsOverTime
}

// const tooltipLabelFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' })
const tooltipLabelFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
const tooltipLabelFormatter = (date: number): string => tooltipLabelFormat.format(date)

const toLocaleString = (value: number): string => value.toLocaleString()

const tooltipStyle: React.CSSProperties = {
    color: 'var(--body-color)',
    border: 'none',
    background: 'var(--body-bg)',
}

const commonAreaProps = {
    isAnimationActive: false,
    strokeWidth: 0,
    stackId: 'stack',
    type: 'stepBefore',
} as const

interface StateDefinition {
    fill: string
    label: string
    sortOrder: number
}

type DisplayableChangesetCounts = Pick<
    ChangesetCountsOverTimeFields,
    'openPending' | 'openChangesRequested' | 'openApproved' | 'closed' | 'merged' | 'draft'
>

const states: Record<keyof DisplayableChangesetCounts, StateDefinition> = {
    draft: { fill: 'var(--text-muted)', label: 'Draft', sortOrder: 5 },
    openPending: { fill: 'var(--warning)', label: 'Awaiting review', sortOrder: 4 },
    openChangesRequested: { fill: 'var(--danger)', label: 'Changes requested', sortOrder: 3 },
    openApproved: { fill: 'var(--success)', label: 'Approved', sortOrder: 2 },
    closed: { fill: 'var(--secondary)', label: 'Closed', sortOrder: 1 },
    merged: { fill: 'var(--merged)', label: 'Merged', sortOrder: 0 },
}

const tooltipItemSorter = ({ dataKey }: TooltipPayload): number =>
    states[dataKey as keyof DisplayableChangesetCounts].sortOrder

/**
 * A burndown chart showing progress of the batch change's changesets.
 */
export const BatchChangeBurndownChart: React.FunctionComponent<Props> = ({
    batchChangeID,
    queryChangesetCountsOverTime = _queryChangesetCountsOverTime,
    width = '100%',
}) => {
    const [includeArchived, setIncludeArchived] = useState<boolean>(false)
    const toggleIncludeArchived = useCallback((): void => setIncludeArchived(previousValue => !previousValue), [])

    const [hiddenStates, setHiddenStates] = useState<Set<keyof DisplayableChangesetCounts>>(new Set())
    const changesetCountsOverTime: ChangesetCountsOverTimeFields[] | undefined = useObservable(
        useMemo(() => queryChangesetCountsOverTime({ batchChange: batchChangeID, includeArchived }), [
            batchChangeID,
            queryChangesetCountsOverTime,
            includeArchived,
        ])
    )

    const dateTickFormatter = useMemo(() => {
        let dateTickFormat = new Intl.DateTimeFormat(undefined, { month: 'long', day: 'numeric' })
        if (changesetCountsOverTime && changesetCountsOverTime?.length > 1) {
            const start = parseISO(changesetCountsOverTime[0].date)
            const end = parseISO(changesetCountsOverTime[changesetCountsOverTime.length - 1].date)
            // If the range spans multiple years, we want to display the year as well.
            if (getYear(start) !== getYear(end)) {
                dateTickFormat = new Intl.DateTimeFormat(undefined, {
                    month: 'short',
                    day: 'numeric',
                    year: '2-digit',
                })
            }
        }
        return (timestamp: number): string => dateTickFormat.format(timestamp)
    }, [changesetCountsOverTime])

    // Is loading.
    if (changesetCountsOverTime === undefined) {
        return (
            <div className="text-center">
                <LoadingSpinner className="icon-inline mx-auto my-4" />
            </div>
        )
    }

    return (
        <div className="d-flex batch-change-burndown-chart__container align-items-center">
            <ResponsiveContainer width={width} height={300} className="test-batches-chart">
                <ComposedChart
                    data={changesetCountsOverTime.map(snapshot => ({ ...snapshot, date: Date.parse(snapshot.date) }))}
                >
                    <XAxis
                        dataKey="date"
                        domain={[
                            changesetCountsOverTime[0].date,
                            changesetCountsOverTime[changesetCountsOverTime.length - 1].date,
                        ]}
                        name="Time"
                        tickFormatter={dateTickFormatter}
                        type="number"
                        stroke="var(--text-muted)"
                        scale="time"
                    />
                    <YAxis
                        tickFormatter={toLocaleString}
                        stroke="var(--text-muted)"
                        type="number"
                        allowDecimals={false}
                        domain={[0, 'dataMax']}
                    />
                    <Tooltip
                        labelFormatter={tooltipLabelFormatter as LabelFormatter}
                        isAnimationActive={false}
                        wrapperStyle={{ border: '1px solid var(--border-color)' }}
                        contentStyle={tooltipStyle}
                        labelStyle={{ fontWeight: 'bold' }}
                        itemStyle={tooltipStyle}
                        itemSorter={tooltipItemSorter}
                    />

                    {Object.entries(states)
                        .sort(([, a], [, b]) => b.sortOrder - a.sortOrder)
                        .filter(([dataKey]) => !hiddenStates.has(dataKey as keyof DisplayableChangesetCounts))
                        .map(([dataKey, state]) => (
                            <Area
                                key={state.sortOrder}
                                dataKey={dataKey}
                                name={state.label}
                                fill={state.fill}
                                // The stroke is used to color the legend, which we
                                // want to match the fill color for each area.
                                stroke={state.fill}
                                {...commonAreaProps}
                            />
                        ))}
                </ComposedChart>
            </ResponsiveContainer>
            <div className="flex-grow-0 ml-2">
                {Object.entries(states).map(([key, state]) => (
                    <LegendLabel
                        key={key}
                        stateKey={key as keyof DisplayableChangesetCounts}
                        label={state.label}
                        fill={state.fill}
                        hiddenStates={hiddenStates}
                        setHiddenStates={setHiddenStates}
                    />
                ))}
                <hr className="flex-grow-1" />
                <IncludeArchivedToggle includeArchived={includeArchived} onToggle={toggleIncludeArchived} />
            </div>
        </div>
    )
}

const LegendLabel: React.FunctionComponent<{
    stateKey: keyof DisplayableChangesetCounts
    label: string
    fill: string
    hiddenStates: Set<keyof DisplayableChangesetCounts>
    setHiddenStates: (
        setter: (currentValue: Set<keyof DisplayableChangesetCounts>) => Set<keyof DisplayableChangesetCounts>
    ) => void
}> = ({ stateKey, label, fill, hiddenStates, setHiddenStates }) => {
    const onChangeCheckbox = useCallback(() => {
        setHiddenStates(current => {
            if (current.has(stateKey)) {
                const newSet = new Set<keyof DisplayableChangesetCounts>(current)
                newSet.delete(stateKey)
                return newSet
            }
            return new Set<keyof DisplayableChangesetCounts>(current).add(stateKey)
        })
    }, [setHiddenStates, stateKey])
    const checked = useMemo(() => !hiddenStates.has(stateKey), [hiddenStates, stateKey])
    return (
        <div className="d-flex align-items-center text-nowrap p-2">
            <div
                // We want to set the fill based on the state config.
                // eslint-disable-next-line react/forbid-dom-props
                style={{
                    backgroundColor: fill,
                }}
                className="batch-change-burndown-chart-legend__color-box mr-2"
            />
            <input id={stateKey} type="checkbox" className="mr-2" checked={checked} onChange={onChangeCheckbox} />
            <label htmlFor={stateKey} className="mb-0">
                {label}
            </label>
        </div>
    )
}

const IncludeArchivedToggle: React.FunctionComponent<{
    includeArchived: boolean
    onToggle: () => void
}> = ({ includeArchived, onToggle }) => (
    <div className="d-flex align-items-center justify-content-between text-nowrap mb-2">
        <label htmlFor="include-archived" className="mb-0 pt-1">
            Include archived
        </label>
        <Toggle
            id="include-archived"
            value={includeArchived}
            onToggle={onToggle}
            title="Include archived changesets"
            className="ml-2"
        />
    </div>
)
