import React, { useCallback, useEffect, useLayoutEffect, useMemo, useState } from 'react'

import { mdiArrowCollapseRight, mdiChevronDown, mdiChevronRight, mdiFilterOutline } from '@mdi/js'
import classNames from 'classnames'
import * as H from 'history'
import { capitalize } from 'lodash'
import { MemoryRouter, useLocation } from 'react-router'

import { HoveredToken } from '@sourcegraph/codeintellify'
import {
    addLineRangeQueryParameter,
    ErrorLike,
    formatSearchParameters,
    lprToRange,
    pluralize,
    toPositionOrRangeQueryParameter,
} from '@sourcegraph/common'
import { Position } from '@sourcegraph/extension-api-classes'
import { useQuery } from '@sourcegraph/http-client'
import { LanguageSpec } from '@sourcegraph/shared/src/codeintel/legacy-extensions/language-specs/language-spec'
import { findLanguageSpec } from '@sourcegraph/shared/src/codeintel/legacy-extensions/language-specs/languages'
import { displayRepoName } from '@sourcegraph/shared/src/components/RepoLink'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { HighlightResponseFormat } from '@sourcegraph/shared/src/graphql-operations'
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
    toPrettyBlobURL,
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
    H4,
    Text,
    Tooltip,
    useSessionStorage,
} from '@sourcegraph/wildcard'

import { ReferencesPanelHighlightedBlobResult, ReferencesPanelHighlightedBlobVariables } from '../graphql-operations'
import { Blob } from '../repo/blob/Blob'
import { Blob as CodeMirrorBlob } from '../repo/blob/CodeMirrorBlob'
import { HoverThresholdProps } from '../repo/RepoContainer'
import { useExperimentalFeatures } from '../stores'
import { parseBrowserRepoURL } from '../util/url'

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
        PlatformContextProps<'urlToFile' | 'requestGraphQL' | 'settings'>,
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

export const ReferencesPanelWithMemoryRouter: React.FunctionComponent<
    React.PropsWithChildren<ReferencesPanelProps>
> = props => (
    // TODO: this won't be working with Router V6
    <MemoryRouter
        // Force router to remount the Panel when external location changes
        key={`${props.externalLocation.pathname}${props.externalLocation.search}${props.externalLocation.hash}`}
        initialEntries={[props.externalLocation]}
    >
        <ReferencesPanel {...props} />
    </MemoryRouter>
)

const ReferencesPanel: React.FunctionComponent<React.PropsWithChildren<ReferencesPanelProps>> = props => {
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
    React.PropsWithChildren<
        ReferencesPanelProps & {
            repoName: string
            line: number
            character: number
            filePath: string
            revision?: string
        }
    >
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

const SearchTokenFindingReferencesList: React.FunctionComponent<
    React.PropsWithChildren<ReferencesPanelPropsWithToken>
> = props => {
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
                <Text className="text-danger">Could not find hovered token.</Text>
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
    React.PropsWithChildren<
        ReferencesPanelPropsWithToken & {
            searchToken: string
            spec: LanguageSpec
            fileContent: string
        }
    >
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

    // The "active URL" is the URL of the highlighted line number in SideBlob,
    // which also influences which item gets highlighted inside
    // CollapsibleLocationList. This URL is persisted to session storage so that
    // it remains sticky between browser reloads and when pressing back/forward
    // in the browser history.
    const [activeURL, setActiveURL] = useSessionStorage<string | undefined>(
        'sideblob-active-url' + sessionStorageKeyFromToken(props.token),
        undefined
    )
    const setActiveLocation = useCallback(
        (location: Location | undefined): void => {
            if (!location) {
                setActiveURL(undefined)
                return
            }
            // Reconstruct the URL instead of using `location.url` to ensure that
            // the commitID is included even when `location.url` doesn't include the
            // commitID (because it's the default revision '').
            const absoluteURL = toPrettyBlobURL({
                filePath: location.file,
                revision: location.commitID,
                repoName: location.repo,
                commitID: location.commitID,
                position: location.range
                    ? {
                          line: location.range.start.line + 1,
                          character: location.range.start.character,
                      }
                    : undefined,
            })
            setActiveURL(absoluteURL)
        },
        [setActiveURL]
    )

    const sideblob = useMemo(() => parseSideBlobProps(activeURL), [activeURL])

    const isActiveLocation = (location: Location): boolean => {
        const result =
            (sideblob?.position &&
                location.range &&
                sideblob.repository === location.repo &&
                sideblob.file === location.file &&
                sideblob.commitID === location.commitID &&
                sideblob.position.line === location.range.start.line) ||
            false
        return result
    }

    // If props.jumpToFirst is true and we finished loading (and have
    // definitions) we select the first definition. We set it as activeLocation
    // and push it to the blobMemoryHistory so the code blob is open.
    useEffect(() => {
        if (props.jumpToFirst && definitions.length > 0) {
            setActiveLocation(definitions[0])
        }
    }, [setActiveLocation, props.jumpToFirst, definitions, setActiveURL])

    const onBlobNav = (url: string): void => {
        // Store the URL that the user promoted even if no definition/reference
        // points to the same line. In case they press "back" in the browser history,
        // the promoted line should be highlighted.
        setActiveURL(url)
        props.externalHistory.push(url)
    }

    const navigateToUrl = (url: string): void => {
        props.externalHistory.push(url)
    }

    // Manual management of the open/closed state of collapsible lists so they
    // stay open/closed across re-renders and re-mounts.
    const location = useLocation()
    const initialCollapseState = useMemo((): Record<string, boolean> => {
        const { viewState } = parseQueryAndHash(location.search, location.hash)
        const state = {
            references: viewState === 'references',
            definitions: viewState === 'definitions',
            implementations: viewState?.startsWith('implementations_') ?? false,
        }
        // If the URL doesn't contain tab=<tab>, we open it (likely because the
        // user clicked on a link in the preview code blob) to show definitions.
        if (!state.references && !state.definitions && !state.implementations) {
            state.definitions = true
        }
        return state
    }, [location])
    const [collapsed, setCollapsed] = useSessionStorage<Record<string, boolean>>(
        'sideblob-collapse-state-' + sessionStorageKeyFromToken(props.token),
        initialCollapseState
    )
    const handleOpenChange = (id: string, isOpen: boolean): void =>
        setCollapsed(previous => ({ ...previous, [id]: isOpen }))

    const isOpen = (id: string): boolean | undefined => collapsed[id]

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
                <div className={classNames('d-flex justify-content-start mt-2', styles.filter)}>
                    <small>
                        <Icon
                            aria-hidden={true}
                            as={canShowSpinner ? LoadingSpinner : undefined}
                            svgPath={!canShowSpinner ? mdiFilterOutline : undefined}
                            size="md"
                            className={styles.filterIcon}
                        />
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
                        setActiveLocation={setActiveLocation}
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
                        setActiveLocation={setActiveLocation}
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
                            setActiveLocation={setActiveLocation}
                            filter={debouncedFilter}
                            isActiveLocation={isActiveLocation}
                            navigateToUrl={navigateToUrl}
                            handleOpenChange={handleOpenChange}
                            isOpen={isOpen}
                        />
                    )}
                </div>
            </div>
            {sideblob && (
                <div data-testid="right-pane" className={classNames('px-0 border-left', styles.rightSubPanel)}>
                    <CardHeader className={classNames('d-flex', styles.cardHeader)}>
                        <small>
                            <Tooltip content="Close code view" placement="left">
                                <Button
                                    aria-label="Close"
                                    onClick={() => setActiveLocation(undefined)}
                                    className={classNames('p-0', styles.sideBlobCollapseButton)}
                                    size="sm"
                                    data-testid="close-code-view"
                                >
                                    <Icon
                                        aria-hidden={true}
                                        size="sm"
                                        svgPath={mdiArrowCollapseRight}
                                        className="border-0"
                                    />
                                </Button>
                            </Tooltip>
                            {activeURL && (
                                <Link
                                    to={activeURL}
                                    onClick={event => {
                                        event.preventDefault()
                                        navigateToUrl(activeURL)
                                    }}
                                    className={styles.sideBlobFilename}
                                >
                                    {sideblob.file}{' '}
                                </Link>
                            )}
                        </small>
                    </CardHeader>
                    <SideBlob {...props} {...sideblob} blobNav={onBlobNav} />
                </div>
            )}
        </div>
    )
}

interface SearchTokenProps {
    searchToken: string
}

interface CollapseProps {
    isOpen: (id: string) => boolean | undefined
    handleOpenChange: (id: string, isOpen: boolean) => void
}

interface ActiveLocationProps {
    isActiveLocation: (location: Location) => boolean
    setActiveLocation: (reference: Location | undefined) => void
}

interface CollapsibleLocationListProps extends ActiveLocationProps, CollapseProps, SearchTokenProps {
    name: string
    locations: Location[]
    filter: string | undefined
    hasMore: boolean
    fetchMore?: () => void
    loadingMore: boolean
    navigateToUrl: (url: string) => void
}

const CollapsibleLocationList: React.FunctionComponent<
    React.PropsWithChildren<CollapsibleLocationListProps>
> = props => {
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
                            <Icon aria-label="Close" svgPath={mdiChevronDown} />
                        ) : (
                            <Icon aria-label="Expand" svgPath={mdiChevronRight} />
                        )}{' '}
                        <H4 className="mb-0">{capitalize(props.name)}</H4>
                        <span className={classNames('ml-2 text-muted small', styles.cardHeaderSmallText)}>
                            ({props.locations.length} displayed{props.hasMore ? ', more available)' : ')'}
                        </span>
                    </CollapseHeader>
                </CardHeader>

                <CollapsePanel id={props.name} data-testid={props.name}>
                    {props.locations.length > 0 ? (
                        <LocationsList
                            searchToken={props.searchToken}
                            locations={props.locations}
                            isActiveLocation={props.isActiveLocation}
                            setActiveLocation={props.setActiveLocation}
                            filter={props.filter}
                            navigateToUrl={props.navigateToUrl}
                            handleOpenChange={(id, isOpen) => props.handleOpenChange(props.name + id, isOpen)}
                            isOpen={id => props.isOpen(props.name + id)}
                        />
                    ) : (
                        <Text className="text-muted pl-2">
                            {props.filter ? (
                                <i>
                                    No {props.name} matching <strong>{props.filter}</strong> found
                                </i>
                            ) : (
                                <i>No {props.name} found</i>
                            )}
                        </Text>
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

interface SideBlobProps extends ReferencesPanelProps {
    activeURL: string
    repository: string
    commitID: string
    file: string
    position?: Position
    blobNav: (url: string) => void
}

function parseSideBlobProps(
    activeURL: string | undefined
): Pick<SideBlobProps, 'activeURL' | 'repository' | 'commitID' | 'file' | 'position'> | undefined {
    if (!activeURL) {
        return undefined
    }
    try {
        const url = parseBrowserRepoURL(activeURL)
        if (!url.repoName || !url.filePath) {
            return undefined
        }

        const position = url.position
            ? new Position(Math.max(url.position.line - 1), Math.max(0, url.position.character - 1))
            : undefined
        return { activeURL, repository: url.repoName, commitID: url.commitID || '', file: url.filePath, position }
    } catch (error) {
        console.error(`failed to parse activeURL ${activeURL}`, error)
        return undefined
    }
}

const SideBlob: React.FunctionComponent<React.PropsWithChildren<SideBlobProps>> = props => {
    const useCodeMirror = useExperimentalFeatures(features => features.enableCodeMirrorFileView ?? false)
    const BlobComponent = useCodeMirror ? CodeMirrorBlob : Blob

    const highlightFormat = useCodeMirror ? HighlightResponseFormat.JSON_SCIP : HighlightResponseFormat.HTML_HIGHLIGHT
    const { data, error, loading } = useQuery<
        ReferencesPanelHighlightedBlobResult,
        ReferencesPanelHighlightedBlobVariables
    >(FETCH_HIGHLIGHTED_BLOB, {
        variables: {
            repository: props.repository,
            commit: props.commitID,
            path: props.file,
            format: highlightFormat,
            html: highlightFormat === HighlightResponseFormat.HTML_HIGHLIGHT,
        },
        // Cache this data but always re-request it in the background when we revisit
        // this page to pick up newer changes.
        fetchPolicy: 'cache-and-network',
        nextFetchPolicy: 'network-only',
    })

    const history = useMemo(() => H.createMemoryHistory(), [])
    const location = useMemo(() => {
        history.replace(props.activeURL)
        return history.location
    }, [history, props.activeURL])

    // If we're loading and haven't received any data yet
    if (loading && !data) {
        return (
            <>
                <LoadingSpinner inline={false} className="mx-auto my-4" />
                <Text alignment="center" className="text-muted">
                    <i>
                        Loading <Code>{props.file}</Code>...
                    </i>
                </Text>
            </>
        )
    }

    // If we received an error before we had received any data
    if (error && !data) {
        return (
            <div>
                <Text className="text-danger">
                    Loading <Code>{props.file}</Code> failed:
                </Text>
                <pre>{error.message}</pre>
            </div>
        )
    }

    // If there weren't any errors and we just didn't receive any data
    if (!data?.repository?.commit?.blob?.highlight) {
        return <>Nothing found</>
    }

    const { html, lsif } = data?.repository?.commit?.blob?.highlight

    // TODO: display a helpful message if syntax highlighting aborted, see https://github.com/sourcegraph/sourcegraph/issues/40841

    return (
        <BlobComponent
            {...props}
            nav={props.blobNav}
            history={history}
            location={location}
            disableStatusBar={true}
            disableDecorations={true}
            wrapCode={true}
            className={styles.sideBlobCode}
            navigateToLineOnAnyClick={true}
            blobInfo={{
                html: html ?? '',
                lsif: lsif ?? '',
                content: data?.repository?.commit?.blob?.content ?? '',
                filePath: props.file,
                repoName: props.repository,
                commitID: props.commitID,
                revision: props.commitID,
                mode: 'lspmode',
            }}
        />
    )
}

interface LocationsListProps extends ActiveLocationProps, CollapseProps, SearchTokenProps {
    locations: Location[]
    filter: string | undefined
    navigateToUrl: (url: string) => void
}

const LocationsList: React.FunctionComponent<React.PropsWithChildren<LocationsListProps>> = ({
    locations,
    isActiveLocation,
    setActiveLocation,
    filter,
    navigateToUrl,
    handleOpenChange,
    isOpen,
    searchToken,
}) => {
    const repoLocationGroups = useMemo(() => buildRepoLocationGroups(locations), [locations])
    const openByDefault = repoLocationGroups.length === 1

    return (
        <>
            {repoLocationGroups.map(group => (
                <CollapsibleRepoLocationGroup
                    key={group.repoName}
                    searchToken={searchToken}
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
    React.PropsWithChildren<
        ActiveLocationProps &
            CollapseProps &
            SearchTokenProps & {
                filter: string | undefined
                navigateToUrl: (url: string) => void
                repoLocationGroup: RepoLocationGroup
                openByDefault: boolean
            }
    >
> = ({
    repoLocationGroup,
    isActiveLocation,
    setActiveLocation,
    navigateToUrl,
    filter,
    openByDefault,
    isOpen,
    handleOpenChange,
    searchToken,
}) => {
    const open = isOpen(repoLocationGroup.repoName) ?? openByDefault

    return (
        <Collapse isOpen={open} onOpenChange={isOpen => handleOpenChange(repoLocationGroup.repoName, isOpen)}>
            <div className={styles.repoLocationGroup}>
                <CollapseHeader
                    as={Button}
                    aria-expanded={open}
                    aria-label={`Repository ${repoLocationGroup.repoName}`}
                    type="button"
                    className={classNames('d-flex justify-content-start w-100', styles.repoLocationGroupHeader)}
                >
                    <Icon aria-hidden="true" svgPath={open ? mdiChevronDown : mdiChevronRight} />
                    <small>
                        <span className={classNames('text-small', styles.repoLocationGroupHeaderRepoName)}>
                            {displayRepoName(repoLocationGroup.repoName)}
                        </span>
                    </small>
                </CollapseHeader>

                <CollapsePanel id={repoLocationGroup.repoName}>
                    {repoLocationGroup.referenceGroups.map(group => (
                        <CollapsibleLocationGroup
                            key={group.path + group.repoName}
                            searchToken={searchToken}
                            group={group}
                            isActiveLocation={isActiveLocation}
                            setActiveLocation={setActiveLocation}
                            filter={filter}
                            handleOpenChange={(id, isOpen) => handleOpenChange(repoLocationGroup.repoName + id, isOpen)}
                            isOpen={id => isOpen(repoLocationGroup.repoName + id)}
                            navigateToUrl={navigateToUrl}
                        />
                    ))}
                </CollapsePanel>
            </div>
        </Collapse>
    )
}

const CollapsibleLocationGroup: React.FunctionComponent<
    React.PropsWithChildren<
        ActiveLocationProps &
            CollapseProps &
            SearchTokenProps & {
                group: LocationGroup
                filter: string | undefined
                navigateToUrl: (url: string) => void
            }
    >
> = ({ group, setActiveLocation, isActiveLocation, filter, isOpen, handleOpenChange }) => {
    // On the first load, update the scroll position towards the active
    // location.  Without this behavior, the scroll position points at the top
    // of the reference panel when reloading the page or going back/forward in
    // the browser history.
    useLayoutEffect(() => {
        const activeLocationElement = document.querySelector('.' + styles.locationActive)
        if (activeLocationElement) {
            activeLocationElement.scrollIntoView({ behavior: 'auto', block: 'center', inline: 'center' })
        }
    }, [])

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
                        <Icon aria-label="Close" svgPath={mdiChevronDown} />
                    ) : (
                        <Icon aria-label="Expand" svgPath={mdiChevronRight} />
                    )}
                    <small className={styles.locationGroupHeaderFilename}>
                        <span>
                            <span
                                aria-label={`File path ${group.path}`}
                                className={classNames('text-small', styles.repoLocationGroupHeaderRepoName)}
                            >
                                {highlighted.length === 2 ? (
                                    <span>
                                        {highlighted[0]}
                                        <mark>{filter}</mark>
                                        {highlighted[1]}
                                    </span>
                                ) : (
                                    group.path
                                )}{' '}
                            </span>
                            <span className={classNames('ml-2 text-muted', styles.cardHeaderSmallText)}>
                                ({group.locations.length}{' '}
                                {pluralize('occurrence', group.locations.length, 'occurrences')})
                            </span>
                        </span>
                        <Badge small={true} variant="secondary" className="ml-4">
                            {locationGroupQuality(group)}
                        </Badge>
                    </small>
                </CollapseHeader>

                <CollapsePanel id={group.repoName + group.path} className="ml-0">
                    <div className={styles.locationContainer}>
                        <ul className="list-unstyled mb-0">
                            {group.locations.map(reference => {
                                const className = isActiveLocation(reference) ? styles.locationActive : ''

                                const locationLine = getLineContent(reference)
                                const lineWithHighlightedToken = locationLine.prePostToken ? (
                                    <>
                                        {locationLine.prePostToken.pre === '' ? (
                                            <></>
                                        ) : (
                                            <Code>{locationLine.prePostToken.pre}</Code>
                                        )}
                                        <mark className="p-0 selection-highlight sourcegraph-document-highlight">
                                            <Code>{locationLine.prePostToken.token}</Code>
                                        </mark>
                                        {locationLine.prePostToken.post === '' ? (
                                            <></>
                                        ) : (
                                            <Code>{locationLine.prePostToken.post}</Code>
                                        )}
                                    </>
                                ) : locationLine.line ? (
                                    <Code>{locationLine.line}</Code>
                                ) : (
                                    ''
                                )

                                return (
                                    <li
                                        key={reference.url}
                                        className={classNames('border-0 rounded-0 mb-0', styles.location, className)}
                                    >
                                        <Button
                                            onClick={event => {
                                                event.preventDefault()
                                                setActiveLocation(reference)
                                            }}
                                            data-test-reference-url={reference.url}
                                            className={styles.locationLink}
                                        >
                                            <span className={styles.locationLinkLineNumber}>
                                                {(reference.range?.start?.line ?? 0) + 1}
                                                {': '}
                                            </span>
                                            {lineWithHighlightedToken}
                                        </Button>
                                    </li>
                                )
                            })}
                        </ul>
                    </div>
                </CollapsePanel>
            </div>
        </Collapse>
    )
}

interface LocationLine {
    prePostToken?: { pre: string; token: string; post: string }
    line?: string
}

export const getLineContent = (location: Location): LocationLine => {
    const range = location.range
    if (range !== undefined) {
        const line = location.lines[range.start.line]

        if (range.end.line === range.start.line) {
            return {
                prePostToken: {
                    pre: line.slice(0, range.start.character).trimStart(),
                    token: line.slice(range.start.character, range.end.character),
                    post: line.slice(range.end.character),
                },
                line: line.trimStart(),
            }
        }
        return {
            prePostToken: {
                pre: line.slice(0, range.start.character).trimStart(),
                token: line.slice(range.start.character),
                post: '',
            },
            line: line.trimStart(),
        }
    }
    return {}
}

const LoadingCodeIntel: React.FunctionComponent<React.PropsWithChildren<{}>> = () => (
    <>
        <LoadingSpinner inline={false} className="mx-auto my-4" />
        <Text alignment="center" className="text-muted">
            <i>Loading code intel ...</i>
        </Text>
    </>
)

const LoadingCodeIntelFailed: React.FunctionComponent<React.PropsWithChildren<{ error: ErrorLike }>> = props => (
    <>
        <div>
            <Text className="text-danger">Loading code intel failed:</Text>
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

function sessionStorageKeyFromToken(token: Token): string {
    return `${token.repoName}@${token.commitID}/${token.filePath}?L${token.line}:${token.character}`
}
