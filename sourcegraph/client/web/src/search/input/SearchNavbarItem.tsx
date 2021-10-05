import { Shortcut } from '@slimsag/react-shortcuts'
import * as H from 'history'
import React, { useCallback, useState, useEffect } from 'react'

import { Form } from '@sourcegraph/branded/src/components/Form'
import { ActivationProps } from '@sourcegraph/shared/src/components/activation/Activation'
import { VersionContextProps } from '@sourcegraph/shared/src/search/util'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { FuzzyFinder } from '@sourcegraph/web/src/components/fuzzyFinder/FuzzyFinder'

import {
    PatternTypeProps,
    CaseSensitivityProps,
    OnboardingTourProps,
    SearchContextInputProps,
    parseSearchURLQuery,
} from '..'
import { AuthenticatedUser } from '../../auth'
import { KEYBOARD_SHORTCUT_FUZZY_FINDER } from '../../keyboardShortcuts/keyboardShortcuts'
import { VersionContext } from '../../schema/site.schema'
import { getExperimentalFeatures } from '../../util/get-experimental-features'
import { submitSearch } from '../helpers'
import { useNavbarQueryState } from '../navbarSearchQueryState'

import { SearchBox } from './SearchBox'

interface Props
    extends ActivationProps,
        PatternTypeProps,
        CaseSensitivityProps,
        SettingsCascadeProps,
        ThemeProps,
        SearchContextInputProps,
        VersionContextProps,
        OnboardingTourProps,
        TelemetryProps {
    authenticatedUser: AuthenticatedUser | null
    location: H.Location
    history: H.History
    isSourcegraphDotCom: boolean
    globbing: boolean
    isSearchAutoFocusRequired?: boolean
    isRepositoryRelatedPage?: boolean
    setVersionContext: (versionContext: string | undefined) => Promise<void>
    availableVersionContexts: VersionContext[] | undefined
}

/**
 * The search item in the navbar
 */
export const SearchNavbarItem: React.FunctionComponent<Props> = (props: Props) => {
    const autoFocus = props.isSearchAutoFocusRequired ?? true
    // This uses the same logic as in Layout.tsx until we have a better solution
    // or remove the search help button
    const isSearchPage = props.location.pathname === '/search' && Boolean(parseSearchURLQuery(props.location.search))
    const [isFuzzyFinderVisible, setIsFuzzyFinderVisible] = useState(false)
    const { queryState, setQueryState } = useNavbarQueryState()

    const onSubmit = useCallback(
        (event?: React.FormEvent): void => {
            event?.preventDefault()
            submitSearch({ ...props, query: queryState.query, source: 'nav' })
        },
        [props, queryState]
    )

    useEffect(() => {
        if (isSearchPage && isFuzzyFinderVisible) {
            setIsFuzzyFinderVisible(false)
        }
    }, [isSearchPage, isFuzzyFinderVisible])

    const { fuzzyFinder, fuzzyFinderCaseInsensitiveFileCountThreshold } = getExperimentalFeatures(
        props.settingsCascade.final
    )

    return (
        <Form
            className="search--navbar-item d-flex align-items-flex-start flex-grow-1 flex-shrink-past-contents"
            onSubmit={onSubmit}
        >
            <SearchBox
                {...props}
                hasGlobalQueryBehavior={true}
                queryState={queryState}
                onChange={setQueryState}
                onSubmit={onSubmit}
                autoFocus={autoFocus}
                hideHelpButton={isSearchPage}
                onHandleFuzzyFinder={setIsFuzzyFinderVisible}
            />
            <Shortcut
                {...KEYBOARD_SHORTCUT_FUZZY_FINDER.keybindings[0]}
                onMatch={() => {
                    setIsFuzzyFinderVisible(true)
                    const input = document.querySelector<HTMLInputElement>('#fuzzy-modal-input')
                    input?.focus()
                    input?.select()
                }}
            />
            {isFuzzyFinderVisible && props.isRepositoryRelatedPage && fuzzyFinder && (
                <FuzzyFinder
                    caseInsensitiveFileCountThreshold={fuzzyFinderCaseInsensitiveFileCountThreshold}
                    setIsVisible={bool => setIsFuzzyFinderVisible(bool)}
                />
            )}
        </Form>
    )
}
