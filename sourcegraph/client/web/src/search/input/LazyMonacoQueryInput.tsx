import React, { Suspense } from 'react'

import { lazyComponent } from '../../util/lazyComponent'

import styles from './LazyMonacoQueryInput.module.scss'
import { MonacoQueryInputProps } from './MonacoQueryInput'

const MonacoQueryInput = lazyComponent(() => import('./MonacoQueryInput'), 'MonacoQueryInput')

/**
 * A plain query input displayed during lazy-loading of the MonacoQueryInput.
 * It has no suggestions, but still allows to type in and submit queries.
 */
export const PlainQueryInput: React.FunctionComponent<
    Pick<MonacoQueryInputProps, 'queryState' | 'autoFocus' | 'onChange'>
> = ({ queryState, autoFocus, onChange }) => {
    const onInputChange = React.useCallback(
        (event: React.ChangeEvent<HTMLInputElement>) => {
            onChange({ query: event.target.value })
        },
        [onChange]
    )
    return (
        <input
            type="text"
            autoFocus={autoFocus}
            className={`form-control text-code ${styles.lazyMonacoQueryInputIntermediateInput}`}
            value={queryState.query}
            onChange={onInputChange}
            spellCheck={false}
        />
    )
}

/**
 * A lazily-loaded {@link MonacoQueryInput}, displaying a read-only query field as a fallback during loading.
 */
export const LazyMonacoQueryInput: React.FunctionComponent<MonacoQueryInputProps> = props => (
    <Suspense fallback={<PlainQueryInput {...props} />}>
        <MonacoQueryInput {...props} />
    </Suspense>
)
