import { FC, ReactNode } from 'react'

import classNames from 'classnames'

import { Button } from '@sourcegraph/wildcard'

import { useUiFeatures } from '../../../hooks'
import { LimitedAccessLabel, useFieldAPI } from '../../index'

import { FormSeriesInput } from './components/form-series-input/FormSeriesInput'
import { SeriesCard } from './components/series-card/SeriesCard'
import { EditableDataSeries } from './types'
import { useEditableSeries } from './use-editable-series'

import styles from './FormSeries.module.scss'

export interface FormSeriesProps {
    seriesField: useFieldAPI<EditableDataSeries[]>
    repositories: string
    showValidationErrorsOnMount: boolean

    /**
     * This field is only needed for specifying a special compute-specific
     * query field description when this component is used on the compute-powered insight.
     * This prop should be removed when we will have a better form series management
     * solution, see https://github.com/sourcegraph/sourcegraph/issues/38236
     */
    queryFieldDescription?: ReactNode
}

export const FormSeries: FC<FormSeriesProps> = props => {
    const { seriesField, showValidationErrorsOnMount, repositories, queryFieldDescription } = props

    const { licensed } = useUiFeatures()
    const { series, changeSeries, editRequest, editCommit, cancelEdit, deleteSeries } = useEditableSeries(seriesField)

    return (
        <ul data-testid="form-series" className="list-unstyled d-flex flex-column">
            {series.map((line, index) =>
                line.edit ? (
                    <FormSeriesInput
                        key={line.id}
                        series={line}
                        showValidationErrorsOnMount={showValidationErrorsOnMount}
                        index={index + 1}
                        cancel={series.length > 1}
                        autofocus={line.autofocus}
                        repositories={repositories}
                        queryFieldDescription={queryFieldDescription}
                        className={classNames('p-3', styles.formSeriesItem)}
                        onSubmit={editCommit}
                        onCancel={() => cancelEdit(line.id)}
                        onChange={(seriesValues, valid) => changeSeries(seriesValues, valid, index)}
                    />
                ) : (
                    line && (
                        <SeriesCard
                            key={line.id}
                            disabled={index >= 10}
                            onEdit={() => editRequest(line.id)}
                            onRemove={() => deleteSeries(line.id)}
                            className={styles.formSeriesItem}
                            {...line}
                        />
                    )
                )
            )}

            {!licensed && (
                <LimitedAccessLabel message="Unlock Code Insights for unlimited data series" className="mx-auto my-3" />
            )}

            <Button
                data-testid="add-series-button"
                type="button"
                onClick={() => editRequest()}
                variant="link"
                disabled={!licensed ? series.length >= 10 : false}
                className={classNames(styles.formSeriesItem, styles.formSeriesAddButton, 'p-3')}
            >
                + Add another data series
            </Button>
        </ul>
    )
}
