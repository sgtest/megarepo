import React, { useCallback, useEffect, useMemo, useState } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import { noop } from 'lodash'
import * as Monaco from 'monaco-editor'
import { BehaviorSubject } from 'rxjs'
import { debounceTime } from 'rxjs/operators'

import {
    StreamingSearchResultsList,
    StreamingSearchResultsListProps,
    useQueryIntelligence,
    useQueryDiagnostics,
} from '@sourcegraph/search-ui'
import { transformSearchQuery } from '@sourcegraph/shared/src/api/client/search'
import { MonacoEditor } from '@sourcegraph/shared/src/components/MonacoEditor'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { LATEST_VERSION } from '@sourcegraph/shared/src/search/stream'
import { fetchStreamSuggestions } from '@sourcegraph/shared/src/search/suggestions'
import { LoadingSpinner, Button, useObservable } from '@sourcegraph/wildcard'

import { PageTitle } from '../components/PageTitle'
import { SearchPatternType } from '../graphql-operations'
import { useExperimentalFeatures } from '../stores'
import { SearchUserNeedsCodeHost } from '../user/settings/codeHosts/OrgUserNeedsCodeHost'

import { parseSearchURLQuery, parseSearchURLPatternType, SearchStreamingProps } from '.'

import styles from './SearchConsolePage.module.scss'

interface SearchConsolePageProps
    extends SearchStreamingProps,
        Omit<StreamingSearchResultsListProps, 'allExpanded' | 'extensionsController' | 'executedQuery'>,
        ExtensionsControllerProps<'executeCommand' | 'extHostAPI'> {
    globbing: boolean
    isMacPlatform: boolean
    history: H.History
    location: H.Location
}

const options: Monaco.editor.IStandaloneEditorConstructionOptions = {
    readOnly: false,
    minimap: {
        enabled: false,
    },
    lineNumbers: 'off',
    fontSize: 14,
    glyphMargin: false,
    overviewRulerBorder: false,
    rulers: [],
    overviewRulerLanes: 0,
    wordBasedSuggestions: false,
    quickSuggestions: false,
    fixedOverflowWidgets: true,
    renderLineHighlight: 'none',
    contextmenu: false,
    links: true,
    // Display the cursor as a 1px line.
    cursorStyle: 'line',
    cursorWidth: 1,
}

export const SearchConsolePage: React.FunctionComponent<React.PropsWithChildren<SearchConsolePageProps>> = props => {
    const {
        globbing,
        streamSearch,
        extensionsController: { extHostAPI: extensionHostAPI },
    } = props

    const showSearchContext = useExperimentalFeatures(features => features.showSearchContext ?? false)

    const searchQuery = useMemo(() => new BehaviorSubject<string>(parseSearchURLQuery(props.location.search) ?? ''), [
        props.location.search,
    ])

    const patternType = useMemo(
        () => parseSearchURLPatternType(props.location.search) || SearchPatternType.structural,
        [props.location.search]
    )

    const triggerSearch = useCallback(() => {
        props.history.push('/search/console?q=' + encodeURIComponent(searchQuery.value))
    }, [props.history, searchQuery])

    const transformedQuery = useMemo(() => {
        const query = parseSearchURLQuery(props.location.search)
        return transformSearchQuery({
            query: query?.replace(/\/\/.*/g, '') || '',
            extensionHostAPIPromise: extensionHostAPI,
        })
    }, [props.location.search, extensionHostAPI])

    // Fetch search results when the `q` URL query parameter changes
    const results = useObservable(
        useMemo(
            () =>
                streamSearch(transformedQuery, {
                    version: LATEST_VERSION,
                    patternType: patternType ?? SearchPatternType.literal,
                    caseSensitive: false,
                    trace: undefined,
                }).pipe(debounceTime(500)),
            [patternType, transformedQuery, streamSearch]
        )
    )

    const sourcegraphSearchLanguageId = useQueryIntelligence(fetchStreamSuggestions, {
        patternType,
        globbing,
        interpretComments: true,
    })

    const [editorInstance, setEditorInstance] = useState<Monaco.editor.IStandaloneCodeEditor>()
    useEffect(() => {
        if (!editorInstance) {
            return
        }
        const disposable = editorInstance.onDidChangeModelContent(() => {
            const query = editorInstance.getValue()
            searchQuery.next(query)
        })
        return () => disposable.dispose()
    }, [editorInstance, searchQuery, props.history])

    useQueryDiagnostics(editorInstance, { patternType, interpretComments: true })

    useEffect(() => {
        if (!editorInstance) {
            return
        }
        const disposable = editorInstance.addAction({
            id: 'submit-on-cmd-enter',
            label: 'Submit search',
            keybindings: [Monaco.KeyMod.CtrlCmd | Monaco.KeyCode.Enter],
            run: () => triggerSearch(),
        })
        return () => disposable.dispose()
    }, [editorInstance, triggerSearch])

    // Register dummy onCompletionSelected handler to prevent console errors
    useEffect(() => {
        const disposable = Monaco.editor.registerCommand('completionItemSelected', noop)
        return () => disposable.dispose()
    }, [])

    return (
        <div className="w-100 p-2">
            <PageTitle title="Search console" />
            <div className="d-flex">
                <div className="flex-1 p-1">
                    <div className="mb-1 d-flex align-items-center justify-content-between">
                        <div />
                        <Button onClick={triggerSearch} variant="primary" size="lg">
                            Search &nbsp; {props.isMacPlatform ? <kbd>⌘</kbd> : <kbd>Ctrl</kbd>}+<kbd>⏎</kbd>
                        </Button>
                    </div>
                    <MonacoEditor
                        {...props}
                        language={sourcegraphSearchLanguageId}
                        options={options}
                        height={600}
                        editorWillMount={noop}
                        onEditorCreated={setEditorInstance}
                        value={searchQuery.value}
                    />
                </div>
                <div className={classNames('flex-1 p-1', styles.results)}>
                    {results &&
                        (results.state === 'loading' ? (
                            <LoadingSpinner />
                        ) : (
                            <StreamingSearchResultsList
                                {...props}
                                allExpanded={false}
                                results={results}
                                showSearchContext={showSearchContext}
                                assetsRoot={window.context?.assetsRoot || ''}
                                renderSearchUserNeedsCodeHost={user => <SearchUserNeedsCodeHost user={user} />}
                                executedQuery={props.location.search}
                            />
                        ))}
                </div>
            </div>
        </div>
    )
}
