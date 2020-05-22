import { from, merge, Observable, Unsubscribable } from 'rxjs'
import { switchMap } from 'rxjs/operators'
import { EndpointPair, PlatformContext } from '../../platform/context'
import { InitData } from '../extension/extensionHost'
import { createExtensionHostClientConnection } from './connection'
import { Services } from './services'

export interface ExtensionHostClient extends Unsubscribable {
    /**
     * Closes the connection to the extension host and stops the controller from reestablishing new
     * connections.
     */
    unsubscribe(): void
}

/**
 * Creates a client to communicate with an extension host.
 *
 * @param extensionHostEndpoint An observable that emits the connection to the extension host each time a new
 * connection is established.
 */
export function createExtensionHostClient(
    services: Services,
    extensionHostEndpoint: Observable<EndpointPair>,
    initData: InitData,
    platformContext: PlatformContext
): ExtensionHostClient {
    const client = extensionHostEndpoint.pipe(
        switchMap(endpoints =>
            from(createExtensionHostClientConnection(endpoints, services, initData, platformContext)).pipe(
                switchMap(client => merge([client], new Observable<never>(() => client)))
            )
        )
    )
    return client.subscribe()
}
