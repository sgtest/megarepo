import { Remote, ProxyMarked } from 'comlink'
import { Unsubscribable } from 'rxjs'
import { DocumentHighlight } from 'sourcegraph'

import { Contributions, Evaluated, Raw, TextDocumentPositionParameters, HoverMerged } from '@sourcegraph/client-api'
import { MaybeLoadingResult } from '@sourcegraph/codeintellify'
import * as clientType from '@sourcegraph/extension-api-types'
import { GraphQLResult } from '@sourcegraph/http-client'

import type { ReferenceContext } from '../codeintel/legacy-extensions/api'
import { Occurrence } from '../codeintel/scip'
import { ConfiguredExtension } from '../extensions/extension'
import { SettingsCascade } from '../settings/settings'

import { SettingsEdit } from './client/services/settings'
import { ExecutableExtension } from './extension/activation'
import { ProxySubscribable } from './extension/api/common'
import { ContributionOptions } from './extension/extensionHostApi'
import { ExtensionViewer, TextDocumentData, ViewerData, ViewerId, ViewerUpdate } from './viewerTypes'

export interface ScipParameters {
    referenceOccurrence: Occurrence
    documentOccurrences: Occurrence[]
}

/**
 * This is exposed from the extension host thread to the main thread
 * e.g. for communicating  direction "main -> ext host"
 * Note this API object lives in the extension host thread
 */
export interface FlatExtensionHostAPI {
    /**
     * Updates the settings exposed to extensions.
     */
    syncSettingsData: (data: Readonly<SettingsCascade<object>>) => void

    // Workspace
    addWorkspaceRoot: (root: clientType.WorkspaceRoot) => void
    getWorkspaceRoots: () => ProxySubscribable<clientType.WorkspaceRoot[]>
    removeWorkspaceRoot: (uri: string) => void

    setSearchContext: (searchContext: string | undefined) => void

    // Languages
    getHover: (parameters: TextDocumentPositionParameters) => ProxySubscribable<MaybeLoadingResult<HoverMerged | null>>
    getDocumentHighlights: (parameters: TextDocumentPositionParameters) => ProxySubscribable<DocumentHighlight[]>
    getDefinition: (
        parameters: TextDocumentPositionParameters,
        scipParameters?: ScipParameters
    ) => ProxySubscribable<MaybeLoadingResult<clientType.Location[]>>
    getReferences: (
        parameters: TextDocumentPositionParameters,
        context: ReferenceContext,
        scipParameters?: ScipParameters
    ) => ProxySubscribable<MaybeLoadingResult<clientType.Location[]>>
    getLocations: (
        id: string,
        parameters: TextDocumentPositionParameters
    ) => ProxySubscribable<MaybeLoadingResult<clientType.Location[]>>

    hasReferenceProvidersForDocument: (parameters: TextDocumentPositionParameters) => ProxySubscribable<boolean>

    // CONTEXT + CONTRIBUTIONS

    /**
     * Sets the given context keys and values.
     * If a value is `null`, the context key is removed.
     *
     * @param update Object with context keys as values
     */
    updateContext: (update: { [k: string]: unknown }) => void

    /**
     * Register contributions and return an unsubscribable that deregisters the contributions.
     * Any expressions in the contributions will be parsed in the extension host.
     */
    registerContributions: (rawContributions: Raw<Contributions>) => Unsubscribable & ProxyMarked

    /**
     * Returns an observable that emits all contributions (merged) evaluated in the current model
     * (with the optional scope). It emits whenever there is any change.
     *
     * @template T Extra allowed property value types for the {@link Context} value. See
     * {@link Context}'s `T` type parameter for more information.
     * @param scope The scope in which contributions are fetched. A scope can be a sub-component of
     * the UI that defines its own context keys, such as the hover, which stores useful loading and
     * definition/reference state in its scoped context keys.
     * @param extraContext Extra context values to use when computing the contributions. Properties
     * in this object shadow (take precedence over) properties in the global context for this
     * computation.
     */
    getContributions: <T>(contributionOptions?: ContributionOptions<T>) => ProxySubscribable<Evaluated<Contributions>>

    // TEXT DOCUMENTS
    addTextDocumentIfNotExists: (textDocumentData: TextDocumentData) => void

    // VIEWERS
    getActiveViewComponentChanges: () => ProxySubscribable<ExtensionViewer | undefined>

    getActiveCodeEditorPosition: () => ProxySubscribable<TextDocumentPositionParameters | null>

    /**
     * Add a viewer.
     *
     * @param viewer The description of the viewer to add.
     * @returns The added code viewer (which must be passed as the first argument to other
     * viewer methods to operate on this viewer).
     */
    addViewerIfNotExists(viewer: ViewerData): ViewerId

    /**
     * Emits whenever a viewer is added or removed.
     */
    viewerUpdates: () => ProxySubscribable<ViewerUpdate>

    /**
     * Sets the selections for a CodeEditor.
     *
     * @param codeEditor The editor for which to set the selections.
     * @param selections The new selections to apply.
     * @throws if no editor exists with the given editor ID.
     * @throws if the editor ID is not a CodeEditor.
     */
    setEditorSelections(codeEditor: ViewerId, selections: clientType.Selection[]): void

    /**
     * Removes a viewer.
     * Also removes the corresponding model if no other viewer is referencing it.
     *
     * @param viewer The viewer to remove.
     */
    removeViewer(viewer: ViewerId): void

    /**
     * Emits true when the initial batch of extensions have been loaded.
     */
    haveInitialExtensionsLoaded: () => ProxySubscribable<boolean>

    getActiveExtensions: () => ProxySubscribable<ConfiguredExtension[]>
}

/**
 * This is exposed from the main thread to the extension host thread"
 * e.g. for communicating  direction "ext host -> main"
 * Note this API object lives in the main thread
 */
export interface MainThreadAPI {
    /**
     * Applies a settings update from extensions.
     */
    applySettingsEdit: (edit: SettingsEdit) => Promise<void>

    /**
     * GraphQL request API
     */
    requestGraphQL: (request: string, variables: any) => Promise<GraphQLResult<any>>

    // Commands
    executeCommand: (command: string, args: any[]) => Promise<any>
    registerCommand: (
        name: string,
        command: Remote<((...args: any) => any) & ProxyMarked>
    ) => Unsubscribable & ProxyMarked

    getEnabledExtensions: () => ProxySubscribable<(ConfiguredExtension | ExecutableExtension)[]>

    /**
     * Log an event (by sending it to the server).
     */
    logEvent: (eventName: string, eventProperties?: any) => void

    /**
     * Log messages from extensions in the main thread. Makes it easier to debug extensions for applications
     * in which extensions run in a different page from the main thread
     * (e.g. browser extensions, where extensions run in the background page).
     */
    logExtensionMessage(message?: any, ...optionalParameters: any[]): void
}
