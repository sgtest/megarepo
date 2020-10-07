/* eslint-disable no-sync */
import * as comlink from 'comlink'
import * as clientType from '@sourcegraph/extension-api-types'
import { Unsubscribable } from 'rxjs'
import {
    CompletionItemProvider,
    DefinitionProvider,
    DocumentSelector,
    Location,
    LocationProvider,
    ReferenceProvider,
} from 'sourcegraph'
import { ClientLanguageFeaturesAPI } from '../../client/api/languageFeatures'
import { ReferenceParams, TextDocumentPositionParams } from '../../protocol'
import { syncSubscription } from '../../util'
import { toProxyableSubscribable } from './common'
import { ExtensionDocuments } from './documents'
import { fromLocation, toPosition, fromDocumentSelector } from './types'

/** @internal */
export class ExtensionLanguageFeatures {
    constructor(private proxy: comlink.Remote<ClientLanguageFeaturesAPI>, private documents: ExtensionDocuments) {}

    public registerDefinitionProvider(selector: DocumentSelector, provider: DefinitionProvider): Unsubscribable {
        const providerFunction: comlink.Local<
            Parameters<ClientLanguageFeaturesAPI['$registerDefinitionProvider']>[1]
        > = comlink.proxy(async ({ textDocument, position }: TextDocumentPositionParams) =>
            toProxyableSubscribable(
                provider.provideDefinition(await this.documents.getSync(textDocument.uri), toPosition(position)),
                toLocations
            )
        )
        return syncSubscription(
            this.proxy.$registerDefinitionProvider(fromDocumentSelector(selector), providerFunction)
        )
    }

    public registerReferenceProvider(selector: DocumentSelector, provider: ReferenceProvider): Unsubscribable {
        const providerFunction: comlink.Local<
            Parameters<ClientLanguageFeaturesAPI['$registerReferenceProvider']>[1]
        > = comlink.proxy(async ({ textDocument, position, context }: ReferenceParams) =>
            toProxyableSubscribable(
                provider.provideReferences(
                    await this.documents.getSync(textDocument.uri),
                    toPosition(position),
                    context
                ),
                toLocations
            )
        )
        return syncSubscription(this.proxy.$registerReferenceProvider(fromDocumentSelector(selector), providerFunction))
    }

    public registerLocationProvider(
        idString: string,
        selector: DocumentSelector,
        provider: LocationProvider
    ): Unsubscribable {
        const providerFunction: comlink.Local<
            Parameters<ClientLanguageFeaturesAPI['$registerLocationProvider']>[2]
        > = comlink.proxy(async ({ textDocument, position }: TextDocumentPositionParams) =>
            toProxyableSubscribable(
                provider.provideLocations(await this.documents.getSync(textDocument.uri), toPosition(position)),
                toLocations
            )
        )
        return syncSubscription(
            this.proxy.$registerLocationProvider(
                idString,
                fromDocumentSelector(selector),
                comlink.proxy(providerFunction)
            )
        )
    }

    public registerCompletionItemProvider(
        selector: DocumentSelector,
        provider: CompletionItemProvider
    ): Unsubscribable {
        const providerFunction: comlink.Local<
            Parameters<ClientLanguageFeaturesAPI['$registerCompletionItemProvider']>[1]
        > = comlink.proxy(async ({ textDocument, position }: TextDocumentPositionParams) =>
            toProxyableSubscribable(
                provider.provideCompletionItems(await this.documents.getSync(textDocument.uri), toPosition(position)),
                items => items
            )
        )
        return syncSubscription(
            this.proxy.$registerCompletionItemProvider(fromDocumentSelector(selector), providerFunction)
        )
    }
}

function toLocations(result: Location[] | Location | null | undefined): clientType.Location[] {
    return result ? (Array.isArray(result) ? result : [result]).map(location => fromLocation(location)) : []
}
