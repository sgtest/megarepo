import React from 'react'

import { KeyboardShortcut } from '@sourcegraph/shared/src/keyboardShortcuts'
import { ThemeProps } from '@sourcegraph/shared/src/theme'

import { CopyQueryButtonProps, SearchContextProps } from '..'
import { VersionContextDropdown } from '../../nav/VersionContextDropdown'
import { VersionContext } from '../../schema/site.schema'
import { QueryState, submitSearch } from '../helpers'

import { LazyMonacoQueryInput } from './LazyMonacoQueryInput'
import styles from './SearchBox.module.scss'
import { SearchContextDropdown } from './SearchContextDropdown'
import { Toggles, TogglesProps } from './toggles/Toggles'

export interface SearchBoxProps
    extends Omit<TogglesProps, 'navbarSearchQuery'>,
        ThemeProps,
        Omit<
            SearchContextProps,
            'convertVersionContextToSearchContext' | 'isSearchContextSpecAvailable' | 'fetchSearchContext'
        >,
        CopyQueryButtonProps {
    isSourcegraphDotCom: boolean // significant for query suggestions
    queryState: QueryState
    onChange: (newState: QueryState) => void
    onSubmit: () => void
    onFocus?: () => void
    onCompletionItemSelected?: () => void
    onSuggestionsInitialized?: (actions: { trigger: () => void }) => void
    autoFocus?: boolean
    keyboardShortcutForFocus?: KeyboardShortcut
    submitSearchOnSearchContextChange?: boolean
    setVersionContext: (versionContext: string | undefined) => Promise<void>
    availableVersionContexts: VersionContext[] | undefined

    /** Whether globbing is enabled for filters. */
    globbing: boolean

    /** Whether to additionally highlight or provide hovers for tokens, e.g., regexp character sets. */
    enableSmartQuery: boolean

    /** Whether comments are parsed and highlighted */
    interpretComments?: boolean

    /** Don't show the version contexts dropdown. */
    hideVersionContexts?: boolean
}

export const SearchBox: React.FunctionComponent<SearchBoxProps> = props => {
    const { queryState } = props

    return (
        <div className={styles.searchBox}>
            {!props.hideVersionContexts && (
                <VersionContextDropdown
                    history={props.history}
                    caseSensitive={props.caseSensitive}
                    patternType={props.patternType}
                    navbarSearchQuery={queryState.query}
                    versionContext={props.versionContext}
                    setVersionContext={props.setVersionContext}
                    availableVersionContexts={props.availableVersionContexts}
                    selectedSearchContextSpec={props.selectedSearchContextSpec}
                />
            )}
            {props.showSearchContext && (
                <SearchContextDropdown query={queryState.query} submitSearch={submitSearch} {...props} />
            )}
            <div className={`${styles.searchBoxFocusContainer} flex-shrink-past-contents`}>
                <LazyMonacoQueryInput {...props} />
                <Toggles {...props} navbarSearchQuery={queryState.query} className={styles.searchBoxToggleContainer} />
            </div>
        </div>
    )
}
