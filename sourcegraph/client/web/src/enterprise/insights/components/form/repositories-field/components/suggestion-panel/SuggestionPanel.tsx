import { ComboboxList, ComboboxOption, ComboboxOptionText } from '@reach/combobox'
import SourceRepositoryIcon from 'mdi-react/SourceRepositoryIcon'
import React from 'react'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { isErrorLike } from '@sourcegraph/common'
import { LoadingSpinner } from '@sourcegraph/wildcard'

import styles from './SuggestionPanel.module.scss'

interface SuggestionsPanelProps {
    value: string | null
    suggestions?: Error | RepositorySuggestion[]
}

interface RepositorySuggestion {
    id: string
    name: string
}

/**
 * Renders suggestion panel for repositories combobox component.
 */
export const SuggestionsPanel: React.FunctionComponent<SuggestionsPanelProps> = props => {
    const { value, suggestions } = props

    if (suggestions === undefined) {
        return (
            <div className={styles.loadingPanel}>
                <LoadingSpinner inline={false} />
            </div>
        )
    }

    if (isErrorLike(suggestions)) {
        return <ErrorAlert className="m-1" error={suggestions} data-testid="repository-suggestions-error" />
    }

    const searchValue = value ?? ''
    const isValueEmpty = searchValue.trim() === ''

    return (
        <ComboboxList className={styles.suggestionsList}>
            {suggestions.map(suggestion => (
                <ComboboxOption className={styles.suggestionsListItem} key={suggestion.id} value={suggestion.name}>
                    <SourceRepositoryIcon className="mr-1" size="1rem" />
                    <ComboboxOptionText />
                </ComboboxOption>
            ))}

            {!isValueEmpty && !suggestions.length && (
                <span className={styles.suggestionsListItem}>No results found</span>
            )}
        </ComboboxList>
    )
}
