import React, { DOMAttributes, useRef, useState } from 'react'

import { mdiFilterOutline } from '@mdi/js'
import classNames from 'classnames'

import { Button, createRectangle, Popover, PopoverContent, PopoverTrigger, Position, Icon } from '@sourcegraph/wildcard'

import { Insight, InsightFilters } from '../../../../../../core'
import { FormChangeEvent, SubmissionResult } from '../../../../../form/hooks/useForm'
import {
    DrillDownInsightCreationForm,
    DrillDownInsightCreationFormValues,
    DrillDownFiltersFormValues,
    DrillDownInsightFilters,
    FilterSectionVisualMode,
    hasActiveFilters,
} from '../drill-down-filters-panel'

import styles from './DrillDownFiltersPopover.module.scss'

const POPOVER_PADDING = createRectangle(0, 0, 5, 5)
interface DrillDownFiltersPopoverProps {
    isOpen: boolean
    initialFiltersValue: InsightFilters
    originalFiltersValue: InsightFilters
    anchor: React.RefObject<HTMLElement>
    insight: Insight
    onFilterChange: (filters: InsightFilters) => void
    onFilterSave: (filters: InsightFilters) => void
    onInsightCreate: (values: DrillDownInsightCreationFormValues) => SubmissionResult
    onVisibilityChange: (open: boolean) => void
}

// To prevent grid layout position change animation. Attempts to drag
// the filter panel should not trigger react-grid-layout events.
const handleMouseDown: DOMAttributes<HTMLElement>['onMouseDown'] = event => event.stopPropagation()

export enum DrillDownFiltersStep {
    Filters = 'filters',
    ViewCreation = 'view-creation',
}

const STEP_STYLES = {
    [DrillDownFiltersStep.Filters]: styles.popoverWithFilters,
    [DrillDownFiltersStep.ViewCreation]: styles.popoverWithViewCreation,
}

export const DrillDownFiltersPopover: React.FunctionComponent<
    React.PropsWithChildren<DrillDownFiltersPopoverProps>
> = props => {
    const {
        isOpen,
        anchor,
        insight,
        initialFiltersValue,
        originalFiltersValue,
        onVisibilityChange,
        onFilterChange,
        onFilterSave,
        onInsightCreate,
    } = props

    // By default always render filters mode
    const [step, setStep] = useState(DrillDownFiltersStep.Filters)
    const targetButtonReference = useRef<HTMLButtonElement>(null)
    const isFiltered = hasActiveFilters(initialFiltersValue)

    const handleFilterChange = (event: FormChangeEvent<DrillDownFiltersFormValues>): void => {
        if (event.valid) {
            onFilterChange(event.values)
        }
    }

    const handleCreateInsight = (values: DrillDownInsightCreationFormValues): void => {
        setStep(DrillDownFiltersStep.Filters)
        // eslint-disable-next-line @typescript-eslint/no-floating-promises
        onInsightCreate(values)
    }

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
                <Icon
                    className={styles.filterIcon}
                    svgPath={mdiFilterOutline}
                    inline={false}
                    aria-hidden={true}
                    height="1rem"
                    width="1rem"
                />
            </PopoverTrigger>

            <PopoverContent
                targetPadding={POPOVER_PADDING}
                constrainToScrollParents={true}
                position={Position.rightStart}
                aria-label="Drill-down filters panel"
                onMouseDown={handleMouseDown}
                className={classNames(styles.popover, STEP_STYLES[step])}
            >
                {step === DrillDownFiltersStep.Filters && (
                    <DrillDownInsightFilters
                        initialValues={initialFiltersValue}
                        originalValues={originalFiltersValue}
                        visualMode={FilterSectionVisualMode.CollapseSections}
                        seriesCount={insight.seriesCount}
                        onFiltersChange={handleFilterChange}
                        onFilterSave={onFilterSave}
                        onCreateInsightRequest={() => setStep(DrillDownFiltersStep.ViewCreation)}
                    />
                )}

                {step === DrillDownFiltersStep.ViewCreation && (
                    <DrillDownInsightCreationForm
                        onCreateInsight={handleCreateInsight}
                        onCancel={() => setStep(DrillDownFiltersStep.Filters)}
                    />
                )}
            </PopoverContent>
        </Popover>
    )
}
