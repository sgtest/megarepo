import { from, Subscription } from 'rxjs'
import { distinctUntilChanged, map } from 'rxjs/operators'
import { ContextValues } from 'sourcegraph'
import { Connection } from '../protocol/jsonrpc2/connection'
import { Tracer } from '../protocol/jsonrpc2/trace'
import { ClientCodeEditor } from './api/codeEditor'
import { ClientCommands } from './api/commands'
import { ClientConfiguration } from './api/configuration'
import { ClientContext } from './api/context'
import { ClientDocuments } from './api/documents'
import { ClientExtensions } from './api/extensions'
import { ClientLanguageFeatures } from './api/languageFeatures'
import { ClientRoots } from './api/roots'
import { Search } from './api/search'
import { ClientViews } from './api/views'
import { ClientWindows } from './api/windows'
import { applyContextUpdate } from './context/context'
import { Services } from './services'
import {
    MessageActionItem,
    ShowInputParams,
    ShowMessageParams,
    ShowMessageRequestParams,
} from './services/notifications'

export interface ExtensionHostClientConnection {
    /**
     * Sets or unsets the tracer to use for logging all of this client's messages to/from the
     * extension host.
     */
    setTracer(tracer: Tracer | null): void

    /**
     * Closes the connection to and terminates the extension host.
     */
    unsubscribe(): void
}

/**
 * An activated extension.
 */
export interface ActivatedExtension {
    /**
     * The extension's extension ID (which uniquely identifies it among all activated extensions).
     */
    id: string

    /**
     * Deactivate the extension (by calling its "deactivate" function, if any).
     */
    deactivate(): void | Promise<void>
}

export function createExtensionHostClientConnection(
    connection: Connection,
    services: Services
): ExtensionHostClientConnection {
    const subscription = new Subscription()

    connection.onRequest('ping', () => 'pong')

    subscription.add(new ClientConfiguration<any>(connection, services.settings))
    subscription.add(
        new ClientContext(connection, (updates: ContextValues) =>
            services.context.data.next(applyContextUpdate(services.context.data.value, updates))
        )
    )
    subscription.add(
        new ClientWindows(
            connection,
            from(services.model.model).pipe(
                map(({ visibleViewComponents }) => visibleViewComponents),
                distinctUntilChanged()
            ),
            (params: ShowMessageParams) => services.notifications.showMessages.next({ ...params }),
            (params: ShowMessageRequestParams) =>
                new Promise<MessageActionItem | null>(resolve => {
                    services.notifications.showMessageRequests.next({ ...params, resolve })
                }),
            (params: ShowInputParams) =>
                new Promise<string | null>(resolve => {
                    services.notifications.showInputs.next({ ...params, resolve })
                })
        )
    )
    subscription.add(new ClientViews(connection, services.views, services.textDocumentLocations))
    subscription.add(new ClientCodeEditor(connection, services.textDocumentDecoration))
    subscription.add(
        new ClientDocuments(
            connection,
            from(services.model.model).pipe(
                map(
                    ({ visibleViewComponents }) =>
                        visibleViewComponents && visibleViewComponents.map(({ item }) => item)
                ),
                distinctUntilChanged()
            )
        )
    )
    subscription.add(
        new ClientLanguageFeatures(
            connection,
            services.textDocumentHover,
            services.textDocumentDefinition,
            services.textDocumentTypeDefinition,
            services.textDocumentImplementation,
            services.textDocumentReferences,
            services.textDocumentLocations
        )
    )
    subscription.add(new Search(connection, services.queryTransformer))
    subscription.add(new ClientCommands(connection, services.commands))
    subscription.add(
        new ClientRoots(
            connection,
            from(services.model.model).pipe(
                map(({ roots }) => roots),
                distinctUntilChanged()
            )
        )
    )
    subscription.add(new ClientExtensions(connection, services.extensions))

    return {
        setTracer: tracer => {
            connection.trace(tracer)
        },
        unsubscribe: () => subscription.unsubscribe(),
    }
}
