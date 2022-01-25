import * as Comlink from 'comlink'
import { print } from 'graphql'
import { BehaviorSubject, from, Observable } from 'rxjs'

import { GraphQLResult } from '@sourcegraph/http-client'
import { wrapRemoteObservable } from '@sourcegraph/shared/src/api/client/api/common'
import { PlatformContext } from '@sourcegraph/shared/src/platform/context'
import { TelemetryService } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { ExtensionCoreAPI } from '../../contract'

import { vscodeTelemetryService } from './telemetryService'

export interface VSCodePlatformContext
    extends Pick<
        PlatformContext,
        | 'updateSettings'
        | 'settings'
        | 'getGraphQLClient'
        | 'showMessage'
        | 'showInputBox'
        | 'sideloadedExtensionURL'
        | 'getScriptURLForExtension'
        | 'getStaticExtensions'
        | 'telemetryService'
        | 'clientApplication'
    > {
    // Ensure telemetryService is non-nullable.
    telemetryService: TelemetryService
    requestGraphQL: <R, V = object>(options: {
        request: string
        variables: V
        mightContainPrivateInfo: boolean
        overrideAccessToken?: string
    }) => Observable<GraphQLResult<R>>
}

export function createPlatformContext(extensionCoreAPI: Comlink.Remote<ExtensionCoreAPI>): VSCodePlatformContext {
    const context: VSCodePlatformContext = {
        requestGraphQL({ request, variables, overrideAccessToken }) {
            return from(extensionCoreAPI.requestGraphQL(request, variables, overrideAccessToken))
        },
        // TODO add true Apollo Client support for v2
        getGraphQLClient: () =>
            Promise.resolve({
                watchQuery: ({ variables, query }) =>
                    from(extensionCoreAPI.requestGraphQL(print(query), variables)) as any,
            }),
        settings: wrapRemoteObservable(extensionCoreAPI.observeSourcegraphSettings()),
        // TODO: implement GQL mutation, settings refresh (called by extensions, impl w/ ext. host).
        updateSettings: () => Promise.resolve(),
        telemetryService: vscodeTelemetryService,
        sideloadedExtensionURL: new BehaviorSubject<string | null>(null),
        clientApplication: 'other', // TODO add 'vscode-extension' to `clientApplication`,
        getScriptURLForExtension: () => undefined,
        // TODO showMessage
        // TODO showInputBox
    }

    return context
}

export interface WebviewPageProps {
    extensionCoreAPI: Comlink.Remote<ExtensionCoreAPI>
    platformContext: VSCodePlatformContext
    theme: 'theme-dark' | 'theme-light'
    instanceURL: string
}
