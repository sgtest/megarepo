import React, { DOMAttributes, useRef } from 'react'

import classNames from 'classnames'
import FilterOutlineIcon from 'mdi-react/FilterOutlineIcon'

import { Button, createRectangle, Popover, PopoverContent, PopoverTrigger, Position } from '@sourcegraph/wildcard'

import { InsightFilters } from '../../../../../../core'
import { SubmissionResult } from '../../../../../form/hooks/useForm'
import { hasActiveFilters } from '../drill-down-filters-panel/components/drill-down-filters-form/DrillDownFiltersForm'
import { DrillDownInsightCreationFormValues } from '../drill-down-filters-panel/components/drill-down-insight-creation-form/DrillDownInsightCreationForm'
import { DrillDownFiltersPanel } from '../drill-down-filters-panel/DrillDownFiltersPanel'

import styles from './DrillDownFiltersPopover.module.scss'

const POPOVER_PADDING = createRectangle(0, 0, 5, 5)
interface DrillDownFiltersPopoverProps {
    isOpen: boolean
    initialFiltersValue: InsightFilters
    originalFiltersValue: InsightFilters
    anchor: React.RefObject<HTMLElement>
    onFilterChange: (filters: InsightFilters) => void
    onFilterSave: (filters: InsightFilters) => void
    onInsightCreate: (values: DrillDownInsightCreationFormValues) => SubmissionResult
    onVisibilityChange: (open: boolean) => void
}

// To prevent grid layout position change animation. Attempts to drag
// the filter panel should not trigger react-grid-layout events.
const handleMouseDown: DOMAttributes<HTMLElement>['onMouseDown'] = event => event.stopPropagation()

export const DrillDownFiltersPopover: React.FunctionComponent<DrillDownFiltersPopoverProps> = props => {
    const {
        isOpen,
        anchor,
        initialFiltersValue,
        originalFiltersValue,
        onVisibilityChange,
        onFilterChange,
        onFilterSave,
        onInsightCreate,
    } = props

    const targetButtonReference = useRef<HTMLButtonElement>(null)
    const isFiltered = hasActiveFilters(initialFiltersValue)

    return (
        <Popover isOpen={isOpen} anchor={anchor} onOpenChange={event => onVisibilityChange(event.isOpen)}>
            <PopoverTrigger
                as={Button}
                ref={targetButtonReference}
                variant="icon"
                type="button"
                aria-label={isFiltered ? 'Active filters' : 'Filters'}
                className={classNames('btn-icon p-1', styles.filterButton, {
                    [styles.filterButtonWithOpenPanel]: isOpen,
                    [styles.filterButtonActive]: isFiltered,
                })}
            >
                <FilterOutlineIcon className={styles.filterIcon} size="1rem" />
            </PopoverTrigger>

            <PopoverContent
                targetPadding={POPOVER_PADDING}
                constrainToScrollParents={true}
                position={Position.rightStart}
                aria-label="Drill-down filters panel"
                onMouseDown={handleMouseDown}
                className={styles.popover}
            >
                <DrillDownFiltersPanel
                    initialFiltersValue={initialFiltersValue}
                    originalFiltersValue={originalFiltersValue}
                    onFiltersChange={onFilterChange}
                    onFilterSave={onFilterSave}
                    onInsightCreate={onInsightCreate}
                />
            </PopoverContent>
        </Popover>
    )
}
