import { proxy, Remote } from 'comlink'
import { noop, sortBy } from 'lodash'
import { BehaviorSubject, EMPTY, ReplaySubject } from 'rxjs'
import { debounceTime, mapTo } from 'rxjs/operators'
import * as sourcegraph from 'sourcegraph'

import { asError } from '@sourcegraph/common'
import { Location, MarkupKind, Position, Range, Selection } from '@sourcegraph/extension-api-classes'

import { ClientAPI } from '../client/api/api'
import { syncRemoteSubscription } from '../util'

import { createStatusBarItemType } from './api/codeEditor'
import { proxySubscribable } from './api/common'
import { createDecorationType } from './api/decorations'
import { DocumentHighlightKind } from './api/documentHighlights'
import { InitData, updateContext } from './extensionHost'
import { NotificationType, PanelViewData } from './extensionHostApi'
import { ExtensionHostState } from './extensionHostState'
import { addWithRollback } from './util'

export interface InitResult {
    configuration: sourcegraph.ConfigurationService
    workspace: typeof sourcegraph['workspace']
    commands: typeof sourcegraph['commands']
    search: typeof sourcegraph['search']
    languages: typeof sourcegraph['languages'] & {
        // Backcompat definitions that were removed from sourcegraph.d.ts but are still defined (as
        // noops with a log message), to avoid completely breaking extensions that use them.
        registerTypeDefinitionProvider: any
        registerImplementationProvider: any
    }
    graphQL: typeof sourcegraph['graphQL']
    content: typeof sourcegraph['content']
    app: typeof sourcegraph['app']
}

/**
 * Creates a factory function to create extension API objects. This factory function should be called
 * when an extension is activated and the resulting extension API object should be passed to `replaceAPIRequire`.
 *
 * Methods shared between all API instances are implemented when the factory function is created.
 * Methods scoped to an API instance (e.g. `sourcegraph.app.log`) are implemented when the factory function is called.
 */
export function createExtensionAPIFactory(
    state: ExtensionHostState,
    clientAPI: Remote<ClientAPI>,
    initData: Pick<InitData, 'clientApplication' | 'sourcegraphURL'>
): (
    extensionID: string
) => typeof sourcegraph & {
    // Backcompat definitions that were removed from sourcegraph.d.ts but are still defined (as
    // noops with a log message), to avoid completely breaking extensions that use them.
    languages: {
        registerTypeDefinitionProvider: any
        registerImplementationProvider: any
    }
} {
    // Configuration
    const getConfiguration = <C extends object>(): sourcegraph.Configuration<C> => {
        const snapshot = state.settings.value.final as Readonly<C>

        const configuration: sourcegraph.Configuration<C> & { toJSON: any } = {
            value: snapshot,
            get: key => snapshot[key],
            update: (key, value) => clientAPI.applySettingsEdit({ path: [key as string | number], value }),
            toJSON: () => snapshot,
        }
        return configuration
    }
    const configuration: typeof sourcegraph['configuration'] = Object.assign(state.settings.pipe(mapTo(undefined)), {
        get: getConfiguration,
    })

    // Workspace
    const workspace: typeof sourcegraph['workspace'] = {
        get textDocuments() {
            return [...state.textDocuments.values()]
        },
        get roots() {
            return state.roots.value
        },
        get versionContext() {
            return undefined
        },
        get searchContext() {
            return state.searchContext
        },
        onDidOpenTextDocument: state.openedTextDocuments.asObservable(),
        openedTextDocuments: state.openedTextDocuments.asObservable(),
        onDidChangeRoots: state.roots.pipe(mapTo(undefined)),
        rootChanges: state.rootChanges.asObservable(),
        versionContextChanges: EMPTY,
        searchContextChanges: state.searchContextChanges.asObservable(),
    }

    const createProgressReporter = async (
        options: sourcegraph.ProgressOptions
        // `showProgress` returned a promise when progress reporters were created
        // in the main thread. continue to return promise for backward compatibility
        // eslint-disable-next-line @typescript-eslint/require-await
    ): Promise<sourcegraph.ProgressReporter> => {
        // There's no guarantee that UI consumers have subscribed to the progress observable
        // by the time that an extension reports progress, so replay the latest report on subscription.
        const progressSubject = new ReplaySubject<sourcegraph.Progress>(1)

        // progress notifications have to be proxied since the observable
        // `progress` property cannot be cloned
        state.progressNotifications.next(
            proxy({
                baseNotification: {
                    message: options.title,
                    type: NotificationType.Log,
                },
                progress: proxySubscribable(progressSubject.asObservable()),
            })
        )

        // return ProgressReporter, which exposes a subset of Subject methods to extensions
        return {
            next: (progress: sourcegraph.Progress) => {
                progressSubject.next(progress)
            },
            error: (value: any) => {
                const error = asError(value)
                progressSubject.error({
                    message: error.message,
                    name: error.name,
                    stack: error.stack,
                })
            },
            complete: () => {
                progressSubject.complete()
            },
        }
    }

    // App
    const window: sourcegraph.Window = {
        get visibleViewComponents(): sourcegraph.ViewComponent[] {
            const entries = [...state.viewComponents.entries()]
            return sortBy(entries, 0).map(([, viewComponent]) => viewComponent)
        },
        get activeViewComponent(): sourcegraph.ViewComponent | undefined {
            return state.activeViewComponentChanges.value
        },
        activeViewComponentChanges: state.activeViewComponentChanges.asObservable(),
        showNotification: (message, type) => {
            state.plainNotifications.next({ message, type })
        },
        withProgress: async (options, task) => {
            const reporter = await createProgressReporter(options)
            try {
                const result = task(reporter)
                reporter.complete()
                return await result
            } catch (error) {
                reporter.error(error)
                throw error
            }
        },

        showProgress: options => createProgressReporter(options),
        showMessage: message => clientAPI.showMessage(message),
        showInputBox: options => clientAPI.showInputBox(options),
    }

    const app: typeof sourcegraph['app'] = {
        get activeWindow() {
            return window
        },
        activeWindowChanges: new BehaviorSubject(window).asObservable(),
        get windows() {
            return [window]
        },
        registerFileDecorationProvider: (provider: sourcegraph.FileDecorationProvider): sourcegraph.Unsubscribable =>
            addWithRollback(state.fileDecorationProviders, provider),
        createPanelView: id => {
            const panelViewData = new BehaviorSubject<PanelViewData>({
                id,
                title: '',
                content: '',
                component: null,
                priority: 0,
                selector: null,
            })

            const panelView: sourcegraph.PanelView = {
                get title() {
                    return panelViewData.value.title
                },
                set title(title: string) {
                    panelViewData.next({ ...panelViewData.value, title })
                },
                get content() {
                    return panelViewData.value.content
                },
                set content(content: string) {
                    panelViewData.next({ ...panelViewData.value, content })
                },
                get component() {
                    return panelViewData.value.component
                },
                set component(component: { locationProvider: string } | null) {
                    panelViewData.next({ ...panelViewData.value, component })
                },
                get priority() {
                    return panelViewData.value.priority
                },
                set priority(priority: number) {
                    panelViewData.next({ ...panelViewData.value, priority })
                },
                get selector() {
                    return panelViewData.value.selector
                },
                set selector(selector: sourcegraph.DocumentSelector | null) {
                    panelViewData.next({ ...panelViewData.value, selector })
                },
                unsubscribe: () => {
                    subscription.unsubscribe()
                },
            }

            // Batch updates from same tick
            const subscription = addWithRollback(state.panelViews, panelViewData.pipe(debounceTime(0)))

            return panelView
        },
        registerViewProvider: (id, provider) => {
            switch (provider.where) {
                case 'insightsPage':
                    return addWithRollback(state.insightsPageViewProviders, { id, viewProvider: provider })

                case 'directory':
                    return addWithRollback(state.directoryViewProviders, { id, viewProvider: provider })

                case 'global/page':
                    return addWithRollback(state.globalPageViewProviders, { id, viewProvider: provider })

                case 'homepage':
                    return addWithRollback(state.homepageViewProviders, { id, viewProvider: provider })
            }
        },
        createDecorationType,
        createStatusBarItemType,
        // `log` is implemented on extension activation
        log: noop,
    }

    // Commands
    const commands: typeof sourcegraph['commands'] = {
        executeCommand: (command, ...args) => clientAPI.executeCommand(command, args),
        registerCommand: (command, callback) =>
            syncRemoteSubscription(clientAPI.registerCommand(command, proxy(callback))),
    }

    // Search
    const search: typeof sourcegraph['search'] = {
        registerQueryTransformer: transformer => addWithRollback(state.queryTransformers, transformer),
    }

    const languages: InitResult['languages'] = {
        registerHoverProvider: (
            selector: sourcegraph.DocumentSelector,
            provider: sourcegraph.HoverProvider
        ): sourcegraph.Unsubscribable => addWithRollback(state.hoverProviders, { selector, provider }),
        registerDocumentHighlightProvider: (
            selector: sourcegraph.DocumentSelector,
            provider: sourcegraph.DocumentHighlightProvider
        ): sourcegraph.Unsubscribable => addWithRollback(state.documentHighlightProviders, { selector, provider }),
        registerDefinitionProvider: (
            selector: sourcegraph.DocumentSelector,
            provider: sourcegraph.DefinitionProvider
        ): sourcegraph.Unsubscribable => addWithRollback(state.definitionProviders, { selector, provider }),
        registerReferenceProvider: (
            selector: sourcegraph.DocumentSelector,
            provider: sourcegraph.ReferenceProvider
        ): sourcegraph.Unsubscribable => addWithRollback(state.referenceProviders, { selector, provider }),
        registerLocationProvider: (
            id: string,
            selector: sourcegraph.DocumentSelector,
            provider: sourcegraph.LocationProvider
        ): sourcegraph.Unsubscribable =>
            addWithRollback(state.locationProviders, { selector, provider: { id, provider } }),

        // These were removed, but keep them here so that calls from old extensions do not throw
        // an exception and completely break.
        registerTypeDefinitionProvider: () => {
            console.warn(
                'sourcegraph.languages.registerTypeDefinitionProvider was removed. Use sourcegraph.languages.registerLocationProvider instead.'
            )
            return { unsubscribe: () => undefined }
        },
        registerImplementationProvider: () => {
            console.warn(
                'sourcegraph.languages.registerImplementationProvider was removed. Use sourcegraph.languages.registerLocationProvider instead.'
            )
            return { unsubscribe: () => undefined }
        },
        registerCompletionItemProvider: (): sourcegraph.Unsubscribable => {
            console.warn('sourcegraph.languages.registerCompletionItemProvider was removed.')
            return { unsubscribe: () => undefined }
        },
    }

    // GraphQL
    const graphQL: typeof sourcegraph['graphQL'] = {
        execute: (query, variables) => clientAPI.requestGraphQL(query, variables),
    }

    // Content
    const content: typeof sourcegraph['content'] = {
        registerLinkPreviewProvider: (urlMatchPattern: string, provider: sourcegraph.LinkPreviewProvider) =>
            addWithRollback(state.linkPreviewProviders, { urlMatchPattern, provider }),
    }

    // For debugging/tests.
    const sync = async (): Promise<void> => {
        await clientAPI.ping()
    }

    return function extensionAPIFactory(extensionID) {
        return {
            URI: URL,
            Position,
            Range,
            Selection,
            Location,
            MarkupKind,
            NotificationType,
            DocumentHighlightKind,
            app: {
                ...app,
                log: (...data) => {
                    if (state.activeLoggers.has(extensionID)) {
                        // Use a light gray background to differentiate extension ID from the message
                        clientAPI
                            .logExtensionMessage(`🧩 %c${extensionID}`, 'background-color: lightgrey;', ...data)
                            .catch(error => {
                                console.error('Error sending extension message to main thread:', error)
                            })
                    }
                },
            },

            workspace,
            configuration,

            languages,

            search,
            commands,
            graphQL,
            content,

            internal: {
                sync: () => sync(),
                updateContext: (updates: sourcegraph.ContextValues) => updateContext(updates, state),
                sourcegraphURL: new URL(initData.sourcegraphURL),
                clientApplication: initData.clientApplication,
            },
        }
    }
}
