import classNames from 'classnames'
import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { Dropdown, DropdownMenu, DropdownToggle } from 'reactstrap'

import { FilterType } from '@sourcegraph/shared/src/search/query/filters'
import { filterExists } from '@sourcegraph/shared/src/search/query/validate'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { SearchContextInputProps } from '..'
import { AuthenticatedUser } from '../../auth'
import { useTemporarySetting } from '../../settings/temporary/useTemporarySetting'
import { SubmitSearchProps } from '../helpers'

import { SearchContextCtaPrompt } from './SearchContextCtaPrompt'
import styles from './SearchContextDropdown.module.scss'
import { SearchContextMenu } from './SearchContextMenu'

export interface SearchContextDropdownProps
    extends Omit<SearchContextInputProps, 'showSearchContext'>,
        TelemetryProps,
        Partial<Pick<SubmitSearchProps, 'submitSearch'>> {
    isSourcegraphDotCom: boolean
    authenticatedUser: AuthenticatedUser | null
    query: string
    className?: string
    onEscapeMenuClose?: () => void
}

export const SearchContextDropdown: React.FunctionComponent<SearchContextDropdownProps> = props => {
    const {
        isSourcegraphDotCom,
        authenticatedUser,
        hasUserAddedRepositories,
        hasUserAddedExternalServices,
        query,
        selectedSearchContextSpec,
        setSelectedSearchContextSpec,
        submitSearch,
        fetchAutoDefinedSearchContexts,
        fetchSearchContexts,
        className,
        telemetryService,
        onEscapeMenuClose,
    } = props

    const [hasUsedNonGlobalContext] = useTemporarySetting('search.usedNonGlobalContext')

    const [isOpen, setIsOpen] = useState(false)
    const toggleOpen = useCallback(() => {
        telemetryService.log('SearchContextDropdownToggled')
        setIsOpen(value => !value)
    }, [telemetryService])

    const isContextFilterInQuery = useMemo(() => filterExists(query, FilterType.context), [query])

    const disabledTooltipText = isContextFilterInQuery ? 'Overridden by query' : ''

    const selectSearchContextSpec = useCallback(
        (spec: string): void => {
            if (submitSearch) {
                submitSearch({
                    source: 'filter',
                    selectedSearchContextSpec: spec,
                })
            } else {
                setSelectedSearchContextSpec(spec)
            }
        },
        [submitSearch, setSelectedSearchContextSpec]
    )

    useEffect(() => {
        if (isOpen && authenticatedUser) {
            // Log search context dropdown view event whenever dropdown is opened, if user is authenticated
            telemetryService.log('SearchContextsDropdownViewed')
        }

        if (isOpen && !authenticatedUser) {
            // Log CTA view event whenver dropdown is opened, if user is not authenticated
            telemetryService.log('SearchResultContextsCTAShown')
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isOpen])

    const onCloseMenu = useCallback(
        (isEscapeKey?: boolean) => {
            if (isEscapeKey) {
                onEscapeMenuClose?.()
            }
            toggleOpen()
        },
        [toggleOpen, onEscapeMenuClose]
    )

    return (
        <Dropdown
            isOpen={isOpen}
            toggle={toggleOpen}
            a11y={false} /* Override default keyboard events in reactstrap */
            className={className}
        >
            <DropdownToggle
                className={classNames(
                    styles.button,
                    'dropdown-toggle',
                    'test-search-context-dropdown',
                    isOpen && styles.buttonOpen
                )}
                color="link"
                disabled={isContextFilterInQuery}
                data-tooltip={disabledTooltipText}
            >
                <code className={classNames('test-selected-search-context-spec', styles.buttonContent)}>
                    <span className="search-filter-keyword">context:</span>
                    {selectedSearchContextSpec?.startsWith('@') ? (
                        <>
                            <span className="search-keyword">@</span>
                            {selectedSearchContextSpec?.slice(1)}
                        </>
                    ) : (
                        selectedSearchContextSpec
                    )}
                </code>
            </DropdownToggle>
            <DropdownMenu positionFixed={true} className={styles.menu}>
                {isSourcegraphDotCom && !hasUserAddedRepositories && !hasUsedNonGlobalContext ? (
                    <SearchContextCtaPrompt
                        telemetryService={telemetryService}
                        authenticatedUser={authenticatedUser}
                        hasUserAddedExternalServices={hasUserAddedExternalServices}
                    />
                ) : (
                    <SearchContextMenu
                        {...props}
                        selectSearchContextSpec={selectSearchContextSpec}
                        fetchAutoDefinedSearchContexts={fetchAutoDefinedSearchContexts}
                        fetchSearchContexts={fetchSearchContexts}
                        closeMenu={onCloseMenu}
                    />
                )}
            </DropdownMenu>
        </Dropdown>
    )
}
