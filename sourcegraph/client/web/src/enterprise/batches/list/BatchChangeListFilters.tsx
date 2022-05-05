import React from 'react'

import { MultiSelect, MultiSelectOption, MultiSelectProps } from '@sourcegraph/wildcard'

import { BatchChangeState } from '../../../graphql-operations'

export const OPEN_STATUS: MultiSelectOption<BatchChangeState> = { label: 'Open', value: BatchChangeState.OPEN }
export const DRAFT_STATUS: MultiSelectOption<BatchChangeState> = { label: 'Draft', value: BatchChangeState.DRAFT }
export const CLOSED_STATUS: MultiSelectOption<BatchChangeState> = { label: 'Closed', value: BatchChangeState.CLOSED }

export const STATUS_OPTIONS: MultiSelectOption<BatchChangeState>[] = [OPEN_STATUS, DRAFT_STATUS, CLOSED_STATUS]
// Drafts are a new feature of severside execution that for now should not be shown if
// execution is not enabled.
const STATUS_OPTIONS_NO_DRAFTS: MultiSelectOption<BatchChangeState>[] = [OPEN_STATUS, CLOSED_STATUS]

interface BatchChangeListFiltersProps
    extends Required<Pick<MultiSelectProps<MultiSelectOption<BatchChangeState>>, 'onChange' | 'value'>> {
    className?: string
    isExecutionEnabled: boolean
}

export const BatchChangeListFilters: React.FunctionComponent<React.PropsWithChildren<BatchChangeListFiltersProps>> = ({
    isExecutionEnabled,
    ...props
}) => (
    <MultiSelect
        {...props}
        options={isExecutionEnabled ? STATUS_OPTIONS : STATUS_OPTIONS_NO_DRAFTS}
        aria-label="Select batch change status to filter."
    />
)
