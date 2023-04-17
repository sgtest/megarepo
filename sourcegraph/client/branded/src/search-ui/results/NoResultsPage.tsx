import React, { useCallback, useEffect } from 'react'

import { mdiClose, mdiOpenInNew } from '@mdi/js'
import classNames from 'classnames'

import { QueryState, SearchContextProps, SearchMode, SubmitSearchParameters } from '@sourcegraph/shared/src/search'
import { NoResultsSectionID as SectionID } from '@sourcegraph/shared/src/settings/temporary/searchSidebar'
import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary/useTemporarySetting'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Button, Link, Icon, H2, H3, Text } from '@sourcegraph/wildcard'

import { QueryExamples } from '../components/QueryExamples'
import { SmartSearchPreview } from '../components/SmartSearchPreview'

import { AnnotatedSearchInput } from './AnnotatedSearchExample'

import styles from './NoResultsPage.module.scss'

interface ContainerProps {
    sectionID?: SectionID
    className?: string
    title: string
    children: React.ReactElement | React.ReactElement[]
    onClose?: (sectionID: SectionID) => void
}

const Container: React.FunctionComponent<React.PropsWithChildren<ContainerProps>> = ({
    sectionID,
    title,
    children,
    onClose,
    className = '',
}) => (
    <div className={classNames(styles.container, className)}>
        <H3 className={styles.title}>
            <span className="flex-1">{title}</span>
            {sectionID && (
                <Button variant="icon" aria-label="Hide Section" onClick={() => onClose?.(sectionID)}>
                    <Icon aria-hidden={true} svgPath={mdiClose} />
                </Button>
            )}
        </H3>
        <div className={styles.content}>{children}</div>
    </div>
)

interface NoResultsPageProps extends TelemetryProps, Pick<SearchContextProps, 'searchContextsEnabled'> {
    isSourcegraphDotCom: boolean
    enableOwnershipSearch: boolean
    showSearchContext: boolean
    showQueryExamples?: boolean
    setQueryState?: (query: QueryState) => void
    searchMode?: SearchMode
    setSearchMode?: (mode: SearchMode) => void
    submitSearch?: (parameters: SubmitSearchParameters) => void
    searchQueryFromURL?: string
    caseSensitive?: boolean
    selectedSearchContextSpec?: string
}

export const NoResultsPage: React.FunctionComponent<React.PropsWithChildren<NoResultsPageProps>> = ({
    searchContextsEnabled,
    telemetryService,
    isSourcegraphDotCom,
    enableOwnershipSearch,
    showSearchContext,
    showQueryExamples,
    setQueryState,
    searchMode,
    setSearchMode,
    submitSearch,
    caseSensitive,
    searchQueryFromURL,
    selectedSearchContextSpec,
}) => {
    const [hiddenSectionIDs, setHiddenSectionIds] = useTemporarySetting('search.hiddenNoResultsSections')

    const onClose = useCallback(
        (sectionID: SectionID) => {
            telemetryService.log('NoResultsPanel', { panelID: sectionID, action: 'closed' })
            setHiddenSectionIds((hiddenSectionIDs = []) =>
                !hiddenSectionIDs.includes(sectionID) ? [...hiddenSectionIDs, sectionID] : hiddenSectionIDs
            )
        },
        [setHiddenSectionIds, telemetryService]
    )

    useEffect(() => {
        telemetryService.logViewEvent('NoResultsPage')
    }, [telemetryService])

    return (
        <div className={styles.root}>
            {searchMode !== SearchMode.SmartSearch &&
                setSearchMode &&
                submitSearch &&
                typeof caseSensitive === 'boolean' &&
                searchQueryFromURL && (
                    <SmartSearchPreview
                        setSearchMode={setSearchMode}
                        submitSearch={submitSearch}
                        caseSensitive={caseSensitive}
                        searchQueryFromURL={searchQueryFromURL}
                    />
                )}

            {showQueryExamples && setQueryState && (
                <>
                    <H3 as={H2}>Search basics</H3>
                    <div className={styles.queryExamplesContainer}>
                        <QueryExamples
                            selectedSearchContextSpec={selectedSearchContextSpec}
                            telemetryService={telemetryService}
                            setQueryState={setQueryState}
                            isSourcegraphDotCom={isSourcegraphDotCom}
                            enableOwnershipSearch={enableOwnershipSearch}
                        />
                    </div>
                </>
            )}
            <div className={styles.panels}>
                <div className="flex-1 flex-shrink-past-contents">
                    {!hiddenSectionIDs?.includes(SectionID.SEARCH_BAR) && (
                        <Container sectionID={SectionID.SEARCH_BAR} title="The search bar" onClose={onClose}>
                            <div className={styles.annotatedSearchInput}>
                                <AnnotatedSearchInput showSearchContext={searchContextsEnabled && showSearchContext} />
                            </div>
                        </Container>
                    )}

                    <Container title="More resources">
                        <Text>Check out the docs for more tips on getting the most from Sourcegraph.</Text>
                        <Text>
                            <Link
                                onClick={() => telemetryService.log('NoResultsMore', { link: 'Docs' })}
                                target="blank"
                                to="https://docs.sourcegraph.com/"
                            >
                                Sourcegraph Docs <Icon svgPath={mdiOpenInNew} aria-label="Open in a new tab" />
                            </Link>
                        </Text>
                    </Container>

                    {hiddenSectionIDs && hiddenSectionIDs.length > 0 && (
                        <Text>
                            Some help panels are hidden.{' '}
                            <Button
                                className="p-0 border-0 align-baseline"
                                onClick={() => {
                                    telemetryService.log('NoResultsPanel', { action: 'showAll' })
                                    setHiddenSectionIds([])
                                }}
                                variant="link"
                            >
                                Show all panels.
                            </Button>
                        </Text>
                    )}
                </div>
            </div>
        </div>
    )
}
