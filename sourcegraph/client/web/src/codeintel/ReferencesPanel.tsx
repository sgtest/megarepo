import React, { useEffect, useMemo, useState } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import { capitalize } from 'lodash'
import ArrowCollapseRightIcon from 'mdi-react/ArrowCollapseRightIcon'
import ChevronDownIcon from 'mdi-react/ChevronDownIcon'
import ChevronRightIcon from 'mdi-react/ChevronRightIcon'
import FilterOutlineIcon from 'mdi-react/FilterOutlineIcon'
import { MemoryRouter, useHistory, useLocation } from 'react-router'

import { HoveredToken } from '@sourcegraph/codeintellify'
import {
    addLineRangeQueryParameter,
    ErrorLike,
    formatSearchParameters,
    lprToRange,
    pluralize,
    toPositionOrRangeQueryParameter,
    toViewStateHash,
} from '@sourcegraph/common'
import { useQuery } from '@sourcegraph/http-client'
import { displayRepoName } from '@sourcegraph/shared/src/components/RepoFileLink'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { getModeFromPath } from '@sourcegraph/shared/src/languages'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import {
    RepoSpec,
    RevisionSpec,
    FileSpec,
    ResolvedRevisionSpec,
    parseQueryAndHash,
} from '@sourcegraph/shared/src/util/url'
import {
    Link,
    LoadingSpinner,
    CardHeader,
    useDebounce,
    Button,
    Input,
    Icon,
    Badge,
    Collapse,
    CollapseHeader,
    CollapsePanel,
    Code,
} from '@sourcegraph/wildcard'

import { ReferencesPanelHighlightedBlobResult, ReferencesPanelHighlightedBlobVariables } from '../graphql-operations'
import { Blob } from '../repo/blob/Blob'
import { HoverThresholdProps } from '../repo/RepoContainer'
import { parseBrowserRepoURL } from '../util/url'

import { findLanguageSpec } from './language-specs/languages'
import { LanguageSpec } from './language-specs/languagespec'
import { Location, LocationGroup, locationGroupQuality, buildRepoLocationGroups, RepoLocationGroup } from './location'
import { FETCH_HIGHLIGHTED_BLOB } from './ReferencesPanelQueries'
import { newSettingsGetter } from './settings'
import { findSearchToken } from './token'
import { useCodeIntel } from './useCodeIntel'
import { useRepoAndBlob } from './useRepoAndBlob'
import { isDefined } from './util/helpers'

import styles from './ReferencesPanel.module.scss'

type Token = HoveredToken & RepoSpec & RevisionSpec & FileSpec & ResolvedRevisionSpec

interface ReferencesPanelProps
    extends SettingsCascadeProps,
        PlatformContextProps<'urlToFile' | 'requestGraphQL' | 'settings' | 'forceUpdateTooltip'>,
        TelemetryProps,
        HoverThresholdProps,
        ExtensionsControllerProps,
        ThemeProps {
    /** Whether to show the first loaded reference in mini code view */
    jumpToFirst?: boolean

    /**
     * The panel runs inside its own MemoryRouter, we keep track of externalHistory
     * so that we're still able to actually navigate within the browser when required
     */
    externalHistory: H.History
    externalLocation: H.Location
}

export const ReferencesPanelWithMemoryRouter: React.FunctionComponent<ReferencesPanelProps> = props => (
    <MemoryRouter
        // Force router to remount the Panel when external location changes
        key={`${props.externalLocation.pathname}${props.externalLocation.search}${props.externalLocation.hash}`}
        initialEntries={[props.externalLocation]}
    >
        <ReferencesPanel {...props} />
    </MemoryRouter>
)

const ReferencesPanel: React.FunctionComponent<ReferencesPanelProps> = props => {
    const location = useLocation()

    const { hash, pathname, search } = location
    const { line, character } = parseQueryAndHash(search, hash)
    const { filePath, repoName, revision } = parseBrowserRepoURL(pathname)

    // If we don't have enough information in the URL, we can't render the panel
    if (!(line && character && filePath)) {
        return null
    }

    const searchParameters = new URLSearchParams(search)
    const jumpToFirst = searchParameters.get('jumpToFirst') === 'true'

    const token = { repoName, line, character, filePath }

    return <RevisionResolvingReferencesList {...props} {...token} revision={revision} jumpToFirst={jumpToFirst} />
}

export const RevisionResolvingReferencesList: React.FunctionComponent<
    ReferencesPanelProps & {
        repoName: string
        line: number
        character: number
        filePath: string
        revision?: string
    }
> = props => {
    const { data, loading, error } = useRepoAndBlob(props.repoName, props.filePath, props.revision)
    if (loading && !data) {
        return <LoadingCodeIntel />
    }

    if (error && !data) {
        return <LoadingCodeIntelFailed error={error} />
    }

    if (!data) {
        return <>Nothing found</>
    }

    const token = {
        repoName: props.repoName,
        line: props.line,
        character: props.character,
        filePath: props.filePath,
        revision: data.revision,
        commitID: data.commitID,
    }

    return (
        <SearchTokenFindingReferencesList
            {...props}
            token={token}
            isFork={data.isFork}
            isArchived={data.isArchived}
            fileContent={data.fileContent}
        />
    )
}

interface ReferencesPanelPropsWithToken extends ReferencesPanelProps {
    token: Token
    isFork: boolean
    isArchived: boolean
    fileContent: string
}

const SearchTokenFindingReferencesList: React.FunctionComponent<ReferencesPanelPropsWithToken> = props => {
    const languageId = getModeFromPath(props.token.filePath)
    const spec = findLanguageSpec(languageId)
    const tokenResult = findSearchToken({
        text: props.fileContent,
        position: {
            line: props.token.line - 1,
            character: props.token.character - 1,
        },
        lineRegexes: spec.commentStyles.map(style => style.lineRegex).filter(isDefined),
        blockCommentStyles: spec.commentStyles.map(style => style.block).filter(isDefined),
        identCharPattern: spec.identCharPattern,
    })

    if (!tokenResult?.searchToken) {
        return (
            <div>
                <p className="text-danger">Could not find hovered token.</p>
            </div>
        )
    }

    return (
        <ReferencesList
            {...props}
            token={props.token}
            searchToken={tokenResult?.searchToken}
            spec={spec}
            fileContent={props.fileContent}
            isFork={props.isFork}
            isArchived={props.isArchived}
        />
    )
}

const SHOW_SPINNER_DELAY_MS = 100

export const ReferencesList: React.FunctionComponent<
    ReferencesPanelPropsWithToken & {
        searchToken: string
        spec: LanguageSpec
        fileContent: string
    }
> = props => {
    const [filter, setFilter] = useState<string>()
    const debouncedFilter = useDebounce(filter, 150)

    useEffect(() => {
        setFilter(undefined)
    }, [props.token])

    const getSetting = newSettingsGetter(props.settingsCascade)

    const {
        data,
        error,
        loading,
        referencesHasNextPage,
        implementationsHasNextPage,
        fetchMoreReferences,
        fetchMoreImplementations,
        fetchMoreReferencesLoading,
        fetchMoreImplementationsLoading,
    } = useCodeIntel({
        variables: {
            repository: props.token.repoName,
            commit: props.token.commitID,
            path: props.token.filePath,
            // On the backend the line/character are 0-indexed, but what we
            // get from hoverifier is 1-indexed.
            line: props.token.line - 1,
            character: props.token.character - 1,
            filter: debouncedFilter || null,
            firstReferences: 100,
            afterReferences: null,
            firstImplementations: 100,
            afterImplementations: null,
        },
        fileContent: props.fileContent,
        searchToken: props.searchToken,
        spec: props.spec,
        isFork: props.isFork,
        isArchived: props.isArchived,
        getSetting,
    })

    // We only show the inline loading message if loading takes longer than
    // SHOW_SPINNER_DELAY_MS milliseconds.
    const [canShowSpinner, setCanShowSpinner] = useState(false)
    useEffect(() => {
        const handle = setTimeout(() => setCanShowSpinner(loading), SHOW_SPINNER_DELAY_MS)
        // In case the component un-mounts before
        return () => clearTimeout(handle)
        // Make sure this effect only runs once
    }, [loading])

    const references = useMemo(() => data?.references.nodes ?? [], [data])
    const definitions = useMemo(() => data?.definitions.nodes ?? [], [data])
    const implementations = useMemo(() => data?.implementations.nodes ?? [], [data])

    // activeLocation is the location that is selected/clicked in the list of
    // definitions/references/implementations.
    const [activeLocation, setActiveLocation] = useState<Location>()
    const isActiveLocation = (location: Location): boolean =>
        activeLocation !== undefined && activeLocation.url === location.url
    // We create an in-memory history here so we don't modify the browser
    // location. This panel is detached from the URL state.
    const blobMemoryHistory = useMemo(() => H.createMemoryHistory(), [])

    // When the token for which we display data changed, we want to reset
    // activeLocation.
    // But only if we are not re-rendering with different token and the code
    // blob already open.
    useEffect(() => {
        if (!props.jumpToFirst) {
            setActiveLocation(undefined)
        }
    }, [props.jumpToFirst, props.token])

    // If props.jumpToFirst is true and we finished loading (and have
    // definitions) we select the first definition. We set it as activeLocation
    // and push it to the blobMemoryHistory so the code blob is open.
    useEffect(() => {
        if (props.jumpToFirst && definitions.length > 0) {
            blobMemoryHistory.push(definitions[0].url)
            setActiveLocation(definitions[0])
        }
    }, [blobMemoryHistory, props.jumpToFirst, definitions])

    // When a user clicks on an item in the list of references, we push it to
    // the memory history for the code blob on the right, so it will jump to &
    // highlight the correct line.
    const onReferenceClick = (location: Location | undefined): void => {
        if (location) {
            blobMemoryHistory.push(location.url)
        }
        setActiveLocation(location)
    }

    // This is the history of the panel, that is inside a memory router
    const panelHistory = useHistory()
    // When we user clicks on a token *inside* the code blob on the right, we
    // update the history for the panel itself, which is inside a memory router.
    //
    // We also '#tab=references' and '?jumpToFirst=true' to the URL.
    //
    // '#tab=references' will cause the panel to show the references of the clicked token,
    // but not navigate the main web app to it.
    //
    // '?jumpToFirst=true' causes the panel to select the first reference and
    // open it in code blob on right.
    const onBlobNav = (url: string): void => {
        // If we're going to navigate inside the same file in the same repo we
        // can optimistically jump to that position in the code blob.
        if (activeLocation !== undefined) {
            const urlToken = tokenFromUrl(url)
            if (urlToken.filePath === activeLocation.file && urlToken.repoName === activeLocation.repo) {
                blobMemoryHistory.push(url)
            }
        }

        panelHistory.push(appendJumpToFirstQueryParameter(url) + toViewStateHash('references'))
    }

    const navigateToUrl = (url: string): void => {
        props.externalHistory.push(url)
    }

    // Manual management of the open/closed state of collapsible lists so they
    // stay open/closed across re-renders and re-mounts.
    const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
    const handleOpenChange = (id: string, isOpen: boolean): void =>
        setCollapsed(previous => ({ ...previous, [id]: isOpen }))
    const isOpen = (id: string): boolean | undefined => collapsed[id]
    // But when the input changes, we reset the collapse state
    useEffect(() => {
        setCollapsed({})
    }, [props.token])

    if (loading && !data) {
        return <LoadingCodeIntel />
    }

    // If we received an error before we had received any data
    if (error && !data) {
        return <LoadingCodeIntelFailed error={error} />
    }

    // If there weren't any errors and we just didn't receive any data
    if (!data) {
        return <>Nothing found</>
    }

    return (
        <div className={classNames('align-items-stretch', styles.panel)}>
            <div className={classNames('px-0', styles.leftSubPanel)}>
                <CardHeader className={classNames('d-flex align-items-center', styles.cardHeader)}>
                    <Code size="base" weight="bold">
                        {props.searchToken}
                    </Code>
                    {canShowSpinner && (
                        <small className="ml-3 text-muted d-flex align-items-center">
                            <Icon as={LoadingSpinner} size="sm" inline={true} className="mr-1" />
                            <i>Loading...</i>
                        </small>
                    )}
                </CardHeader>
                <div className={classNames('d-flex justify-content-start', styles.filter)}>
                    <small>
                        <Icon as={FilterOutlineIcon} size="sm" className={styles.filterIcon} />
                    </small>
                    <Input
                        className={classNames('py-0 my-0 w-100 text-small')}
                        type="text"
                        placeholder="Type to filter by filename"
                        value={filter === undefined ? '' : filter}
                        onChange={event => setFilter(event.target.value)}
                    />
                </div>
                <div className={styles.locationLists}>
                    <CollapsibleLocationList
                        {...props}
                        name="definitions"
                        locations={definitions}
                        hasMore={false}
                        loadingMore={false}
                        filter={debouncedFilter}
                        navigateToUrl={navigateToUrl}
                        isActiveLocation={isActiveLocation}
                        setActiveLocation={onReferenceClick}
                        handleOpenChange={handleOpenChange}
                        isOpen={isOpen}
                    />
                    <CollapsibleLocationList
                        {...props}
                        name="references"
                        locations={references}
                        hasMore={referencesHasNextPage}
                        fetchMore={fetchMoreReferences}
                        loadingMore={fetchMoreReferencesLoading}
                        filter={debouncedFilter}
                        navigateToUrl={navigateToUrl}
                        setActiveLocation={onReferenceClick}
                        isActiveLocation={isActiveLocation}
                        handleOpenChange={handleOpenChange}
                        isOpen={isOpen}
                    />
                    {implementations.length > 0 && (
                        <CollapsibleLocationList
                            {...props}
                            name="implementations"
                            locations={implementations}
                            hasMore={implementationsHasNextPage}
                            fetchMore={fetchMoreImplementations}
                            loadingMore={fetchMoreImplementationsLoading}
                            setActiveLocation={onReferenceClick}
                            filter={debouncedFilter}
                            isActiveLocation={isActiveLocation}
                            navigateToUrl={navigateToUrl}
                            handleOpenChange={handleOpenChange}
                            isOpen={isOpen}
                        />
                    )}
                </div>
            </div>
            {activeLocation !== undefined && (
                <div className={classNames('px-0 border-left', styles.rightSubPanel)}>
                    <CardHeader className={classNames('d-flex', styles.cardHeader)}>
                        <small>
                            <Button
                                onClick={() => setActiveLocation(undefined)}
                                className={classNames('btn-icon p-0', styles.sideBlobCollapseButton)}
                                title="Close code view"
                                data-tooltip="Close code view"
                                data-placement="left"
                                size="sm"
                            >
                                <Icon size="sm" as={ArrowCollapseRightIcon} className="border-0" />
                            </Button>
                            <Link
                                to={activeLocation.url}
                                onClick={event => {
                                    event.preventDefault()
                                    navigateToUrl(activeLocation.url)
                                }}
                                className={styles.sideBlobFilename}
                            >
                                {activeLocation.file}{' '}
                            </Link>
                        </small>
                    </CardHeader>
                    <SideBlob
                        {...props}
                        blobNav={onBlobNav}
                        history={blobMemoryHistory}
                        location={blobMemoryHistory.location}
                        activeLocation={activeLocation}
                    />
                </div>
            )}
        </div>
    )
}

interface CollapseProps {
    isOpen: (id: string) => boolean | undefined
    handleOpenChange: (id: string, isOpen: boolean) => void
}

interface ActiveLocationProps {
    isActiveLocation: (location: Location) => boolean
    setActiveLocation: (reference: Location | undefined) => void
}

interface CollapsibleLocationListProps extends ActiveLocationProps, CollapseProps {
    name: string
    locations: Location[]
    filter: string | undefined
    hasMore: boolean
    fetchMore?: () => void
    loadingMore: boolean
    navigateToUrl: (url: string) => void
}

const CollapsibleLocationList: React.FunctionComponent<CollapsibleLocationListProps> = props => {
    const isOpen = props.isOpen(props.name) ?? true

    return (
        <Collapse isOpen={isOpen} onOpenChange={isOpen => props.handleOpenChange(props.name, isOpen)}>
            <>
                <CardHeader className={styles.cardHeaderBig}>
                    <CollapseHeader
                        as={Button}
                        aria-expanded={props.isOpen(props.name)}
                        type="button"
                        className="d-flex p-0 justify-content-start w-100"
                    >
                        {isOpen ? (
                            <Icon aria-label="Close" as={ChevronDownIcon} />
                        ) : (
                            <Icon aria-label="Expand" as={ChevronRightIcon} />
                        )}{' '}
                        <h4 className="mb-0">{capitalize(props.name)}</h4>
                        <span className={classNames('ml-2 text-muted small', styles.cardHeaderSmallText)}>
                            ({props.locations.length} displayed{props.hasMore ? ', more available)' : ')'}
                        </span>
                    </CollapseHeader>
                </CardHeader>

                <CollapsePanel id={props.name} data-testid={props.name}>
                    {props.locations.length > 0 ? (
                        <LocationsList
                            locations={props.locations}
                            isActiveLocation={props.isActiveLocation}
                            setActiveLocation={props.setActiveLocation}
                            filter={props.filter}
                            navigateToUrl={props.navigateToUrl}
                            handleOpenChange={(id, isOpen) => props.handleOpenChange(props.name + id, isOpen)}
                            isOpen={id => props.isOpen(props.name + id)}
                        />
                    ) : (
                        <p className="text-muted pl-2">
                            {props.filter ? (
                                <i>
                                    No {props.name} matching <strong>{props.filter}</strong> found
                                </i>
                            ) : (
                                <i>No {props.name} found</i>
                            )}
                        </p>
                    )}

                    {props.hasMore &&
                        props.fetchMore !== undefined &&
                        (props.loadingMore ? (
                            <div className="text-center mb-1">
                                <em>Loading more {props.name}...</em>
                                <LoadingSpinner inline={true} />
                            </div>
                        ) : (
                            <div className="text-center mb-1">
                                <Button variant="secondary" onClick={props.fetchMore}>
                                    Load more {props.name}
                                </Button>
                            </div>
                        ))}
                </CollapsePanel>
            </>
        </Collapse>
    )
}

const SideBlob: React.FunctionComponent<
    ReferencesPanelProps & {
        activeLocation: Location

        location: H.Location
        history: H.History
        blobNav: (url: string) => void
    }
> = props => {
    const { data, error, loading } = useQuery<
        ReferencesPanelHighlightedBlobResult,
        ReferencesPanelHighlightedBlobVariables
    >(FETCH_HIGHLIGHTED_BLOB, {
        variables: {
            repository: props.activeLocation.repo,
            commit: props.activeLocation.commitID,
            path: props.activeLocation.file,
        },
        // Cache this data but always re-request it in the background when we revisit
        // this page to pick up newer changes.
        fetchPolicy: 'cache-and-network',
        nextFetchPolicy: 'network-only',
    })

    // If we're loading and haven't received any data yet
    if (loading && !data) {
        return (
            <>
                <LoadingSpinner inline={false} className="mx-auto my-4" />
                <p className="text-muted text-center">
                    <i>
                        Loading <code>{props.activeLocation.file}</code>...
                    </i>
                </p>
            </>
        )
    }

    // If we received an error before we had received any data
    if (error && !data) {
        return (
            <div>
                <p className="text-danger">
                    Loading <code>{props.activeLocation.file}</code> failed:
                </p>
                <pre>{error.message}</pre>
            </div>
        )
    }

    // If there weren't any errors and we just didn't receive any data
    if (!data?.repository?.commit?.blob?.highlight) {
        return <>Nothing found</>
    }

    const { html, aborted } = data?.repository?.commit?.blob?.highlight
    if (aborted) {
        return (
            <p className="text-warning text-center">
                <i>
                    Highlighting <code>{props.activeLocation.file}</code> failed
                </i>
            </p>
        )
    }

    return (
        <Blob
            {...props}
            nav={props.blobNav}
            history={props.history}
            location={props.location}
            disableStatusBar={true}
            wrapCode={true}
            className={styles.sideBlobCode}
            blobInfo={{
                html,
                content: props.activeLocation.content,
                filePath: props.activeLocation.file,
                repoName: props.activeLocation.repo,
                commitID: props.activeLocation.commitID,
                revision: props.activeLocation.commitID,
                mode: 'lspmode',
            }}
        />
    )
}

interface LocationsListProps extends ActiveLocationProps, CollapseProps {
    locations: Location[]
    filter: string | undefined
    navigateToUrl: (url: string) => void
}

const LocationsList: React.FunctionComponent<LocationsListProps> = ({
    locations,
    isActiveLocation,
    setActiveLocation,
    filter,
    navigateToUrl,
    handleOpenChange,
    isOpen,
}) => {
    const repoLocationGroups = useMemo(() => buildRepoLocationGroups(locations), [locations])
    const openByDefault = repoLocationGroups.length === 1

    return (
        <>
            {repoLocationGroups.map(group => (
                <CollapsibleRepoLocationGroup
                    key={group.repoName}
                    repoLocationGroup={group}
                    openByDefault={openByDefault}
                    isActiveLocation={isActiveLocation}
                    setActiveLocation={setActiveLocation}
                    filter={filter}
                    navigateToUrl={navigateToUrl}
                    handleOpenChange={handleOpenChange}
                    isOpen={isOpen}
                />
            ))}
        </>
    )
}

const CollapsibleRepoLocationGroup: React.FunctionComponent<
    ActiveLocationProps &
        CollapseProps & {
            filter: string | undefined
            navigateToUrl: (url: string) => void
            repoLocationGroup: RepoLocationGroup
            openByDefault: boolean
        }
> = ({
    repoLocationGroup,
    isActiveLocation,
    setActiveLocation,
    navigateToUrl,
    filter,
    openByDefault,
    isOpen,
    handleOpenChange,
}) => {
    const repoUrl = `/${repoLocationGroup.repoName}`
    const open = isOpen(repoLocationGroup.repoName) ?? openByDefault

    return (
        <Collapse isOpen={open} onOpenChange={isOpen => handleOpenChange(repoLocationGroup.repoName, isOpen)}>
            <div className={styles.repoLocationGroup}>
                <CollapseHeader
                    as={Button}
                    aria-expanded={open}
                    type="button"
                    className={classNames('d-flex justify-content-start w-100', styles.repoLocationGroupHeader)}
                >
                    {open ? (
                        <Icon aria-label="Close" as={ChevronDownIcon} />
                    ) : (
                        <Icon aria-label="Expand" as={ChevronRightIcon} />
                    )}
                    <small>
                        <Link
                            to={repoUrl}
                            onClick={event => {
                                event.preventDefault()
                                navigateToUrl(repoUrl)
                            }}
                            className={classNames('text-small', styles.repoLocationGroupHeaderRepoName)}
                        >
                            {displayRepoName(repoLocationGroup.repoName)}
                        </Link>
                    </small>
                </CollapseHeader>

                <CollapsePanel id={repoLocationGroup.repoName}>
                    {repoLocationGroup.referenceGroups.map(group => (
                        <CollapsibleLocationGroup
                            key={group.path + group.repoName}
                            group={group}
                            isActiveLocation={isActiveLocation}
                            setActiveLocation={setActiveLocation}
                            filter={filter}
                            handleOpenChange={(id, isOpen) => handleOpenChange(repoLocationGroup.repoName + id, isOpen)}
                            isOpen={id => isOpen(repoLocationGroup.repoName + id)}
                        />
                    ))}
                </CollapsePanel>
            </div>
        </Collapse>
    )
}

const CollapsibleLocationGroup: React.FunctionComponent<
    ActiveLocationProps &
        CollapseProps & {
            group: LocationGroup
            filter: string | undefined
        }
> = ({ group, setActiveLocation, isActiveLocation, filter, isOpen, handleOpenChange }) => {
    let highlighted = [group.path]
    if (filter !== undefined) {
        highlighted = group.path.split(filter)
    }

    const open = isOpen(group.path) ?? true

    return (
        <Collapse isOpen={open} onOpenChange={isOpen => handleOpenChange(group.path, isOpen)}>
            <div className={styles.locationGroup}>
                <CollapseHeader
                    as={Button}
                    aria-expanded={open}
                    type="button"
                    className={classNames(
                        'bg-transparent border-top-0 border-left-0 border-right-0 d-flex justify-content-start w-100',
                        styles.locationGroupHeader
                    )}
                >
                    {open ? (
                        <Icon aria-label="Close" as={ChevronDownIcon} />
                    ) : (
                        <Icon aria-label="Expand" as={ChevronRightIcon} />
                    )}
                    <small className={styles.locationGroupHeaderFilename}>
                        {highlighted.length === 2 ? (
                            <span>
                                {highlighted[0]}
                                <mark>{filter}</mark>
                                {highlighted[1]}
                            </span>
                        ) : (
                            group.path
                        )}{' '}
                        <span className={classNames('ml-2 text-muted small', styles.cardHeaderSmallText)}>
                            ({group.locations.length} {pluralize('occurrence', group.locations.length, 'occurences')})
                        </span>
                        <Badge small={true} variant="secondary" className="ml-4">
                            {locationGroupQuality(group)}
                        </Badge>
                    </small>
                </CollapseHeader>

                <CollapsePanel id={group.repoName + group.path} className="ml-0">
                    <ul className="list-unstyled mb-0">
                        {group.locations.map(reference => {
                            const className = isActiveLocation(reference) ? styles.locationActive : ''

                            return (
                                <li
                                    key={reference.url}
                                    className={classNames('border-0 rounded-0 mb-0', styles.location, className)}
                                >
                                    <Link
                                        as={Button}
                                        onClick={event => {
                                            event.preventDefault()
                                            setActiveLocation(reference)
                                        }}
                                        to={reference.url}
                                        className={styles.locationLink}
                                    >
                                        <span className={styles.locationLinkLineNumber}>
                                            {(reference.range?.start?.line ?? 0) + 1}
                                            {': '}
                                        </span>
                                        <code>{getLineContent(reference)}</code>
                                    </Link>
                                </li>
                            )
                        })}
                    </ul>
                </CollapsePanel>
            </div>
        </Collapse>
    )
}

const getLineContent = (location: Location): string => {
    const range = location.range
    if (range !== undefined) {
        return location.lines[range.start?.line].trim()
    }
    return ''
}

const LoadingCodeIntel: React.FunctionComponent<{}> = () => (
    <>
        <LoadingSpinner inline={false} className="mx-auto my-4" />
        <p className="text-muted text-center">
            <i>Loading code intel ...</i>
        </p>
    </>
)

const LoadingCodeIntelFailed: React.FunctionComponent<{ error: ErrorLike }> = props => (
    <>
        <div>
            <p className="text-danger">Loading code intel failed:</p>
            <pre>{props.error.message}</pre>
        </div>
    </>
)

export function locationWithoutViewState(location: H.Location): H.LocationDescriptorObject {
    const parsedQuery = parseQueryAndHash(location.search, location.hash)
    delete parsedQuery.viewState

    const lineRangeQueryParameter = toPositionOrRangeQueryParameter({ range: lprToRange(parsedQuery) })
    const result = {
        search: formatSearchParameters(
            addLineRangeQueryParameter(new URLSearchParams(location.search), lineRangeQueryParameter)
        ),
        hash: '',
    }
    return result
}

export const appendJumpToFirstQueryParameter = (url: string): string => {
    const newUrl = new URL(url, window.location.href)
    newUrl.searchParams.set('jumpToFirst', 'true')
    return newUrl.pathname + `?${formatSearchParameters(newUrl.searchParams)}` + newUrl.hash
}

const tokenFromUrl = (url: string): { repoName: string; commitID?: string; filePath?: string } => {
    const parsed = new URL(url, window.location.href)

    const { filePath, repoName, commitID } = parseBrowserRepoURL(parsed.pathname)

    return { repoName, filePath, commitID }
}
