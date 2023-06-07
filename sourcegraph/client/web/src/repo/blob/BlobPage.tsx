import React, { useCallback, useEffect, useMemo, useState } from 'react'

import classNames from 'classnames'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import FileAlertIcon from 'mdi-react/FileAlertIcon'
import MapSearchIcon from 'mdi-react/MapSearchIcon'
import { createPortal } from 'react-dom'
import { Navigate, useLocation, useNavigate } from 'react-router-dom'
import { Observable } from 'rxjs'
import { catchError, map, mapTo, startWith, switchMap } from 'rxjs/operators'
import { Optional } from 'utility-types'

import { StreamingSearchResultsListProps } from '@sourcegraph/branded'
import { TabbedPanelContent } from '@sourcegraph/branded/src/components/panel/TabbedPanelContent'
import { NoopEditor } from '@sourcegraph/cody-shared/src/editor'
import { asError, ErrorLike, isErrorLike, basename } from '@sourcegraph/common'
import {
    createActiveSpan,
    reactManualTracer,
    TraceSpanProvider,
    useCurrentSpan,
} from '@sourcegraph/observability-client'
import { FetchFileParameters } from '@sourcegraph/shared/src/backend/file'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { HighlightResponseFormat } from '@sourcegraph/shared/src/graphql-operations'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { SearchContextProps } from '@sourcegraph/shared/src/search'
import { SettingsCascadeProps, useExperimentalFeatures } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'
import { ModeSpec, parseQueryAndHash, RepoFile } from '@sourcegraph/shared/src/util/url'
import {
    Alert,
    Button,
    ButtonLink,
    ErrorMessage,
    Icon,
    LoadingSpinner,
    Panel,
    Text,
    useEventObservable,
    useObservable,
    useSessionStorage,
} from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../auth'
import { CodeIntelligenceProps } from '../../codeintel'
import { FileContentEditor } from '../../cody/components/FileContentEditor'
import { useCodySidebar } from '../../cody/sidebar/Provider'
import { BreadcrumbSetters } from '../../components/Breadcrumbs'
import { HeroPage } from '../../components/HeroPage'
import { PageTitle } from '../../components/PageTitle'
import { useFeatureFlag } from '../../featureFlags/useFeatureFlag'
import { Scalars } from '../../graphql-operations'
import { NotebookProps } from '../../notebooks'
import { copyNotebook, CopyNotebookProps } from '../../notebooks/notebook'
import { OpenInEditorActionItem } from '../../open-in-editor/OpenInEditorActionItem'
import { OwnConfigProps } from '../../own/OwnConfigProps'
import { SearchStreamingProps } from '../../search'
import { useNotepad } from '../../stores'
import { parseBrowserRepoURL, toTreeURL } from '../../util/url'
import { serviceKindDisplayNameAndIcon } from '../actions/GoToCodeHostAction'
import { ToggleBlameAction } from '../actions/ToggleBlameAction'
import { useBlameHunks } from '../blame/useBlameHunks'
import { useBlameVisibility } from '../blame/useBlameVisibility'
import { FilePathBreadcrumbs } from '../FilePathBreadcrumbs'
import { isPackageServiceType } from '../packages/isPackageServiceType'
import { HoverThresholdProps } from '../RepoContainer'
import { RepoHeaderContributionsLifecycleProps } from '../RepoHeader'
import { RepoHeaderContributionPortal } from '../RepoHeaderContributionPortal'

import { ToggleHistoryPanel } from './actions/ToggleHistoryPanel'
import { ToggleLineWrap } from './actions/ToggleLineWrap'
import { ToggleRenderedFileMode } from './actions/ToggleRenderedFileMode'
import { getModeFromURL } from './actions/utils'
import { fetchBlob } from './backend'
import { BlobLoadingSpinner } from './BlobLoadingSpinner'
import { CodeMirrorBlob, type BlobInfo } from './CodeMirrorBlob'
import { GoToRawAction } from './GoToRawAction'
import { HistoryAndOwnBar } from './own/HistoryAndOwnBar'
import { BlobPanel } from './panel/BlobPanel'
import { RenderedFile } from './RenderedFile'
import { TryCodyWidget } from './TryCodyWidget'

import styles from './BlobPage.module.scss'

const SEARCH_NOTEBOOK_FILE_EXTENSION = '.snb.md'
const RenderedNotebookMarkdown = lazyComponent(() => import('./RenderedNotebookMarkdown'), 'RenderedNotebookMarkdown')

interface BlobPageProps
    extends RepoFile,
        ModeSpec,
        RepoHeaderContributionsLifecycleProps,
        SettingsCascadeProps,
        PlatformContextProps,
        TelemetryProps,
        ExtensionsControllerProps,
        HoverThresholdProps,
        BreadcrumbSetters,
        SearchStreamingProps,
        Pick<SearchContextProps, 'searchContextsEnabled'>,
        Pick<StreamingSearchResultsListProps, 'fetchHighlightedFileLineRanges'>,
        Pick<CodeIntelligenceProps, 'codeIntelligenceEnabled' | 'useCodeIntel'>,
        NotebookProps,
        OwnConfigProps {
    authenticatedUser: AuthenticatedUser | null
    isMacPlatform: boolean
    isSourcegraphDotCom: boolean
    repoID?: Scalars['ID']
    repoUrl?: string
    repoServiceType?: string

    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
    className?: string
}

/**
 * Blob data including specific properties used in `BlobPage` but not `Blob`
 */
interface BlobPageInfo extends Optional<BlobInfo, 'commitID'> {
    richHTML: string
    aborted: boolean
}

export const BlobPage: React.FunctionComponent<BlobPageProps> = ({ className, ...props }) => {
    const location = useLocation()
    const navigate = useNavigate()

    const { span } = useCurrentSpan()
    const [wrapCode, setWrapCode] = useState(ToggleLineWrap.getValue())
    let renderMode = getModeFromURL(location)
    const { repoID, repoName, repoServiceType, revision, commitID, filePath, useBreadcrumb, mode } = props
    const enableLazyBlobSyntaxHighlighting = useExperimentalFeatures(
        features => features.enableLazyBlobSyntaxHighlighting ?? true
    )
    const isPackage = useMemo(() => isPackageServiceType(repoServiceType), [repoServiceType])

    const [ownFeatureFlagEnabled] = useFeatureFlag('search-ownership')
    const enableOwnershipPanel = ownFeatureFlagEnabled && props.ownEnabled

    const lineOrRange = useMemo(
        () => parseQueryAndHash(location.search, location.hash),
        [location.search, location.hash]
    )

    // Log view event whenever a new Blob, or a Blob with a different render mode, is visited.
    useEffect(() => {
        props.telemetryService.logViewEvent('Blob', { repoName, filePath })
    }, [repoName, commitID, filePath, renderMode, props.telemetryService])

    useNotepad(
        useMemo(
            () => ({
                type: 'file',
                path: filePath,
                repo: repoName,
                revision,
                // Need to subtract 1 because IHighlightLineRange is 0-based but
                // line information in the URL is 1-based.
                lineRange: lineOrRange.line
                    ? { startLine: lineOrRange.line - 1, endLine: (lineOrRange.endLine ?? lineOrRange.line) - 1 }
                    : null,
            }),
            [filePath, repoName, revision, lineOrRange.line, lineOrRange.endLine]
        )
    )

    useBreadcrumb(
        useMemo(() => {
            if (!filePath) {
                return
            }

            return {
                key: 'filePath',
                className: 'flex-shrink-past-contents',
                element: (
                    // TODO should these be "flattened" all using setBreadcrumb()?
                    <FilePathBreadcrumbs
                        key="path"
                        repoName={repoName}
                        revision={revision}
                        filePath={filePath}
                        isDir={false}
                        telemetryService={props.telemetryService}
                    />
                ),
            }
        }, [filePath, revision, repoName, props.telemetryService])
    )

    const [indexIDsForSnapshotData] = useSessionStorage<{ [repoName: string]: string | undefined }>(
        'blob.preciseIndexIDForSnapshotData',
        {}
    )

    /**
     * Fetches formatted, but un-highlighted, blob content.
     * Intention is to use this whilst we wait for syntax highlighting,
     * so the user has useful content rather than a loading spinner
     */

    const formattedBlobInfoOrError = useObservable(
        useMemo(
            () =>
                createActiveSpan(reactManualTracer, { name: 'formattedBlobInfoOrError', parentSpan: span }, fetchSpan =>
                    fetchBlob({
                        repoName,
                        revision,
                        filePath,
                        format: HighlightResponseFormat.HTML_PLAINTEXT,
                        scipSnapshot: indexIDsForSnapshotData[repoName] !== undefined,
                        visibleIndexID: indexIDsForSnapshotData[repoName],
                    }).pipe(
                        map(blob => {
                            if (blob === null) {
                                return blob
                            }

                            const blobInfo: BlobPageInfo = {
                                content: blob.content,
                                repoName,
                                revision,
                                filePath,
                                mode,
                                // Properties used in `BlobPage` but not `Blob`
                                richHTML: blob.richHTML,
                                aborted: false,
                                lfs: blob.__typename === 'GitBlob' ? blob.lfs : undefined,
                                externalURLs: blob.__typename === 'GitBlob' ? blob.externalURLs : undefined,
                            }

                            fetchSpan.end()

                            return blobInfo
                        })
                    )
                ),
            [filePath, mode, repoName, revision, span, indexIDsForSnapshotData]
        )
    )

    // Bundle latest blob with all other file info to pass to `Blob`
    // Prevents https://github.com/sourcegraph/sourcegraph/issues/14965 by not allowing
    // components to use current file props while blob hasn't updated, since all information
    // is bundled in one object whose creation is blocked by `fetchBlob` emission.
    const [nextFetchWithDisabledTimeout, highlightedBlobInfoOrError] = useEventObservable<
        void,
        BlobPageInfo | null | ErrorLike
    >(
        useCallback(
            (clicks: Observable<void>) =>
                clicks.pipe(
                    mapTo(true),
                    startWith(false),
                    switchMap(disableTimeout =>
                        fetchBlob({
                            repoName,
                            revision,
                            filePath,
                            disableTimeout,
                            format: HighlightResponseFormat.JSON_SCIP,
                            scipSnapshot: indexIDsForSnapshotData[repoName] !== undefined,
                            visibleIndexID: indexIDsForSnapshotData[repoName],
                        })
                    ),
                    map(blob => {
                        if (blob === null) {
                            return blob
                        }

                        const blobInfo: BlobPageInfo = {
                            content: blob.content,
                            lsif: blob.highlight.lsif ?? '',
                            repoName,
                            revision,
                            filePath,
                            mode,
                            // Properties used in `BlobPage` but not `Blob`
                            richHTML: blob.richHTML,
                            aborted: blob.highlight.aborted,
                            lfs: blob.__typename === 'GitBlob' ? blob.lfs : undefined,
                            externalURLs: blob.__typename === 'GitBlob' ? blob.externalURLs : undefined,
                            snapshotData: blob.snapshot,
                        }
                        return blobInfo
                    }),
                    catchError((error): [ErrorLike] => [asError(error)])
                ),
            [repoName, revision, filePath, mode, indexIDsForSnapshotData]
        )
    )

    const blobInfoOrError = enableLazyBlobSyntaxHighlighting
        ? // Fallback to formatted blob whilst we do not have the highlighted blob
          highlightedBlobInfoOrError || formattedBlobInfoOrError
        : highlightedBlobInfoOrError

    const onExtendTimeoutClick = useCallback(
        (event: React.MouseEvent): void => {
            event.preventDefault()
            nextFetchWithDisabledTimeout()
        },
        [nextFetchWithDisabledTimeout]
    )

    const getPageTitle = (): string => {
        const repoNameSplit = repoName.split('/')
        const repoString = repoNameSplit.length > 2 ? repoNameSplit.slice(1).join('/') : repoName
        if (filePath) {
            const fileOrDirectory = filePath.split('/').pop()!
            return `${fileOrDirectory} - ${repoString}`
        }
        return `${repoString}`
    }

    const [isBlameVisible] = useBlameVisibility(isPackage)
    const blameHunks = useBlameHunks({ isPackage, repoName, revision, filePath }, props.platformContext.sourcegraphURL)

    const isSearchNotebook = Boolean(
        blobInfoOrError &&
            !isErrorLike(blobInfoOrError) &&
            blobInfoOrError.filePath.endsWith(SEARCH_NOTEBOOK_FILE_EXTENSION) &&
            props.notebooksEnabled
    )

    const onCopyNotebook = useCallback(
        (props: Omit<CopyNotebookProps, 'title'>) => {
            const title =
                blobInfoOrError && !isErrorLike(blobInfoOrError) ? basename(blobInfoOrError.filePath) : 'Notebook'
            return copyNotebook({ title: `Copy of ${title}`, ...props })
        },
        [blobInfoOrError]
    )

    // If url explicitly asks for a certain rendering mode, renderMode is set to that mode, else it checks:
    // - If file contains richHTML and url does not include a line number: We render in richHTML.
    // - If file does not contain richHTML or the url includes a line number: We render in code view.
    if (!renderMode) {
        renderMode =
            blobInfoOrError && !isErrorLike(blobInfoOrError) && blobInfoOrError.richHTML && !lineOrRange.line
                ? 'rendered'
                : 'code'
    }

    const { setEditorScope } = useCodySidebar()

    useEffect(() => {
        if (!isSearchNotebook && formattedBlobInfoOrError?.richHTML && renderMode === 'rendered') {
            setEditorScope(new FileContentEditor(formattedBlobInfoOrError))
        }
        return () => {
            if (!isSearchNotebook && formattedBlobInfoOrError?.richHTML && renderMode === 'rendered') {
                setEditorScope(new NoopEditor())
            }
        }
    }, [isSearchNotebook, formattedBlobInfoOrError, renderMode, setEditorScope])

    // Always render these to avoid UI jitter during loading when switching to a new file.
    const alwaysRender = (
        <>
            <PageTitle title={getPageTitle()} />
            {!!props.isSourcegraphDotCom && !!props.authenticatedUser && (
                <TryCodyWidget className="mb-4" telemetryService={props.telemetryService} />
            )}
            {window.context.isAuthenticatedUser && (
                <RepoHeaderContributionPortal
                    position="right"
                    priority={112}
                    id="open-in-editor-action"
                    repoHeaderContributionsLifecycleProps={props.repoHeaderContributionsLifecycleProps}
                >
                    {({ actionType }) => (
                        <OpenInEditorActionItem
                            platformContext={props.platformContext}
                            externalServiceType={props.repoServiceType}
                            actionType={actionType}
                            source="repoHeader"
                        />
                    )}
                </RepoHeaderContributionPortal>
            )}
            <RepoHeaderContributionPortal
                position="right"
                priority={111}
                id="toggle-blame-action"
                repoHeaderContributionsLifecycleProps={props.repoHeaderContributionsLifecycleProps}
            >
                {({ actionType }) => (
                    <ToggleBlameAction
                        actionType={actionType}
                        source="repoHeader"
                        renderMode={renderMode}
                        isPackage={isPackage}
                    />
                )}
            </RepoHeaderContributionPortal>
            <RepoHeaderContributionPortal
                position="right"
                priority={20}
                id="toggle-blob-panel"
                repoHeaderContributionsLifecycleProps={props.repoHeaderContributionsLifecycleProps}
            >
                {context => (
                    <ToggleHistoryPanel
                        {...context}
                        key="toggle-blob-panel"
                        location={location}
                        navigate={navigate}
                        isPackage={isPackage}
                    />
                )}
            </RepoHeaderContributionPortal>
            {renderMode === 'code' && (
                <RepoHeaderContributionPortal
                    position="right"
                    priority={99}
                    id="toggle-line-wrap"
                    repoHeaderContributionsLifecycleProps={props.repoHeaderContributionsLifecycleProps}
                >
                    {context => <ToggleLineWrap {...context} key="toggle-line-wrap" onDidUpdate={setWrapCode} />}
                </RepoHeaderContributionPortal>
            )}

            <RepoHeaderContributionPortal
                position="right"
                priority={30}
                id="raw-action"
                repoHeaderContributionsLifecycleProps={props.repoHeaderContributionsLifecycleProps}
            >
                {context => (
                    <GoToRawAction
                        {...context}
                        telemetryService={props.telemetryService}
                        key="raw-action"
                        repoName={repoName}
                        revision={props.revision}
                        filePath={filePath}
                    />
                )}
            </RepoHeaderContributionPortal>
            {enableOwnershipPanel && repoID && (
                <HistoryAndOwnBar repoID={repoID} revision={revision} filePath={filePath} />
            )}
        </>
    )

    if (isErrorLike(blobInfoOrError)) {
        // Be helpful if the URL was actually a tree and redirect.
        // Some extensions may optimistically construct blob URLs because
        // they cannot easily determine eagerly if a file path is a tree or a blob.
        // We don't have error names on GraphQL errors.
        if (/not a blob/i.test(blobInfoOrError.message)) {
            return <Navigate to={toTreeURL(props)} replace={true} />
        }
        return (
            <>
                {alwaysRender}
                <HeroPage icon={AlertCircleIcon} title="Error" subtitle={<ErrorMessage error={blobInfoOrError} />} />
            </>
        )
    }

    if (blobInfoOrError === undefined) {
        // Render placeholder for layout before content is fetched.
        return (
            <div className={classNames(styles.placeholder, className)}>
                {alwaysRender}
                {!enableLazyBlobSyntaxHighlighting && <BlobLoadingSpinner />}
            </div>
        )
    }

    // File not found:
    if (blobInfoOrError === null) {
        return (
            <div className={classNames(styles.placeholder, className)}>
                <HeroPage
                    icon={MapSearchIcon}
                    title="Not found"
                    subtitle={`${filePath} does not exist at this revision.`}
                />
            </div>
        )
    }

    // LFS file:
    if (blobInfoOrError.lfs) {
        const externalUrl = blobInfoOrError.externalURLs?.[0]
        const externalService = externalUrl && serviceKindDisplayNameAndIcon(externalUrl.serviceKind)

        return (
            <div className={classNames(styles.placeholder, className)}>
                <HeroPage
                    icon={FileAlertIcon}
                    title="Stored with Git LFS"
                    subtitle={
                        <div>
                            <Text className={styles.lfsText}>
                                This file is stored in Git Large File Storage and cannot be viewed inside Sourcegraph.
                            </Text>
                            {externalUrl && externalService && (
                                <ButtonLink
                                    variant="secondary"
                                    to={externalUrl.url}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="mt-3"
                                >
                                    <Icon as={externalService.icon} aria-hidden={true} className="mr-1" />
                                    View file on {externalService.displayName}
                                </ButtonLink>
                            )}
                        </div>
                    }
                />
            </div>
        )
    }

    return (
        <div className={className}>
            {alwaysRender}
            {repoID && commitID && <BlobPanel {...props} repoID={repoID} commitID={commitID} isPackage={isPackage} />}
            {blobInfoOrError.richHTML && (
                <RepoHeaderContributionPortal
                    position="right"
                    priority={100}
                    id="toggle-rendered-file-mode"
                    repoHeaderContributionsLifecycleProps={props.repoHeaderContributionsLifecycleProps}
                >
                    {({ actionType }) => (
                        <ToggleRenderedFileMode
                            key="toggle-rendered-file-mode"
                            mode={renderMode || 'rendered'}
                            actionType={actionType}
                        />
                    )}
                </RepoHeaderContributionPortal>
            )}
            {isSearchNotebook && renderMode === 'rendered' && (
                <React.Suspense fallback={<LoadingSpinner />}>
                    <RenderedNotebookMarkdown
                        {...props}
                        markdown={blobInfoOrError.content}
                        onCopyNotebook={onCopyNotebook}
                        exportedFileName={basename(blobInfoOrError.filePath)}
                        className={styles.border}
                    />
                </React.Suspense>
            )}
            {!isSearchNotebook && blobInfoOrError.richHTML && renderMode === 'rendered' && (
                <RenderedFile dangerousInnerHTML={blobInfoOrError.richHTML} className={styles.border} />
            )}
            {!blobInfoOrError.richHTML && blobInfoOrError.aborted && (
                <div>
                    <Alert variant="info">
                        Syntax-highlighting this file took too long. &nbsp;
                        <Button onClick={onExtendTimeoutClick} variant="primary" size="sm">
                            Try again
                        </Button>
                    </Alert>
                </div>
            )}
            {/* Render the (unhighlighted) blob also in the case highlighting timed out */}
            {renderMode === 'code' && commitID && (
                <TraceSpanProvider
                    name="Blob"
                    attributes={{
                        isSearchNotebook,
                        renderMode,
                        enableLazyBlobSyntaxHighlighting,
                    }}
                >
                    <CodeMirrorBlob
                        data-testid="repo-blob"
                        className={classNames(styles.blob, styles.border)}
                        blobInfo={{ ...blobInfoOrError, commitID }}
                        wrapCode={wrapCode}
                        platformContext={props.platformContext}
                        extensionsController={props.extensionsController}
                        settingsCascade={props.settingsCascade}
                        onHoverShown={props.onHoverShown}
                        telemetryService={props.telemetryService}
                        role="region"
                        ariaLabel="File blob"
                        isBlameVisible={isBlameVisible}
                        blameHunks={blameHunks}
                        overrideBrowserSearchKeybinding={true}
                    />
                </TraceSpanProvider>
            )}
            {parseQueryAndHash(location.search, location.hash).viewState &&
                createPortal(
                    <Panel
                        className={styles.panel}
                        position="bottom"
                        defaultSize={350}
                        storageKey="panel-size"
                        ariaLabel="References panel"
                        id="references-panel"
                    >
                        <TabbedPanelContent
                            {...props}
                            repoName={`git://${parseBrowserRepoURL(location.pathname).repoName}`}
                            fetchHighlightedFileLineRanges={props.fetchHighlightedFileLineRanges}
                        />
                    </Panel>,
                    document.querySelector('#references-panel-react-portal')!
                )}
        </div>
    )
}
