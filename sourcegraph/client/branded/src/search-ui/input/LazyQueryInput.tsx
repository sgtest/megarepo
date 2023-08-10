import React, { Suspense } from 'react'

import classNames from 'classnames'

import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'
import { Input } from '@sourcegraph/wildcard'

import type { CodeMirrorQueryInputFacadeProps } from './CodeMirrorQueryInput'

import styles from './LazyQueryInput.module.scss'

/**
 * Minimal interface for external interaction with the editor.
 */
export interface IEditor {
    focus(): void
}

const CodeMirrorQueryInput = lazyComponent(() => import('./CodeMirrorQueryInput'), 'CodeMirrorMonacoFacade')

/**
 * A plain query input displayed during lazy-loading of the MonacoQueryInput.
 * It has no suggestions, but still allows to type in and submit queries.
 */
export const PlainQueryInput: React.FunctionComponent<
    React.PropsWithChildren<
        Pick<LazyQueryInputProps, 'queryState' | 'autoFocus' | 'onChange' | 'className' | 'placeholder'>
    >
> = ({ queryState, autoFocus, onChange, className, placeholder }) => {
    const onInputChange = React.useCallback(
        (event: React.ChangeEvent<HTMLInputElement>) => {
            onChange({ query: event.target.value })
        },
        [onChange]
    )
    return (
        <Input
            autoFocus={autoFocus}
            inputClassName={classNames('text-code', styles.intermediateInput, className)}
            className="w-100"
            value={queryState.query}
            onChange={onInputChange}
            spellCheck={false}
            placeholder={placeholder}
        />
    )
}

export type LazyQueryInputProps = CodeMirrorQueryInputFacadeProps

/**
 * A lazily-loaded {@link MonacoQueryInput}, displaying a read-only query field as a fallback during loading.
 */
export const LazyQueryInput: React.FunctionComponent<LazyQueryInputProps> = ({ ...props }) => (
    <Suspense fallback={<PlainQueryInput {...props} placeholder={props.placeholder} />}>
        <CodeMirrorQueryInput {...props} />
    </Suspense>
)
