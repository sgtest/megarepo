import { combineLatest, merge, Observable, ReplaySubject } from 'rxjs'
import { map, publishReplay, refCount, switchMap, take } from 'rxjs/operators'
import { GraphQLResult, requestGraphQL as requestGraphQLCommon } from '../../../shared/src/graphql/graphql'
import * as GQL from '../../../shared/src/graphql/schema'
import { PlatformContext } from '../../../shared/src/platform/context'
import { mutateSettings, updateSettings } from '../../../shared/src/settings/edit'
import { EMPTY_SETTINGS_CASCADE, gqlToCascade } from '../../../shared/src/settings/settings'
import { LocalStorageSubject } from '../../../shared/src/util/LocalStorageSubject'
import { toPrettyBlobURL } from '../../../shared/src/util/url'
import { ExtensionStorageSubject } from '../browser/ExtensionStorageSubject'
import { background } from '../browser/runtime'
import { observeStorageKey } from '../browser/storage'
import { isInPage } from '../context'
import { CodeHost } from '../libs/code_intelligence'
import { PrivateRepoPublicSourcegraphComError } from '../shared/backend/errors'
import { DEFAULT_SOURCEGRAPH_URL, observeSourcegraphURL } from '../shared/util/context'
import { createExtensionHost } from './extensionHost'
import { editClientSettings, fetchViewerSettings, mergeCascades, storageSettingsCascade } from './settings'

/**
 * Creates the {@link PlatformContext} for the browser extension.
 */
export function createPlatformContext(
    { urlToFile, getContext }: Pick<CodeHost, 'urlToFile' | 'getContext'>,
    sourcegraphURL: string,
    isExtension: boolean
): PlatformContext {
    const updatedViewerSettings = new ReplaySubject<Pick<GQL.ISettingsCascade, 'subjects' | 'final'>>(1)
    const requestGraphQL: PlatformContext['requestGraphQL'] = <T extends GQL.IQuery | GQL.IMutation>({
        request,
        variables,
        mightContainPrivateInfo,
    }: {
        request: string
        variables: {}
        mightContainPrivateInfo: boolean
    }): Observable<GraphQLResult<T>> =>
        observeSourcegraphURL(isExtension).pipe(
            take(1),
            switchMap(sourcegraphURL => {
                if (mightContainPrivateInfo && sourcegraphURL === DEFAULT_SOURCEGRAPH_URL) {
                    // If we can't determine the code host context, assume the current repository is private.
                    const privateRepository = getContext ? getContext().privateRepository : true
                    if (privateRepository) {
                        const nameMatch = request.match(/^\s*(?:query|mutation)\s+(\w+)/)
                        throw new PrivateRepoPublicSourcegraphComError(nameMatch ? nameMatch[1] : '')
                    }
                }
                if (isExtension) {
                    // In the browser extension, send all GraphQL requests from the background page.
                    return background.requestGraphQL<T>({ request, variables })
                }
                return requestGraphQLCommon<T>({
                    request,
                    variables,
                    baseUrl: window.SOURCEGRAPH_URL,
                    headers: {},
                    requestOptions: {
                        crossDomain: true,
                        withCredentials: true,
                        async: true,
                    },
                })
            })
        )

    const context: PlatformContext = {
        /**
         * The active settings cascade.
         *
         * - For unauthenticated users, this is the GraphQL settings plus client settings (which are stored locally
         *   in the browser extension.
         * - For authenticated users, this is just the GraphQL settings (client settings are ignored to simplify
         *   the UX).
         */
        settings: combineLatest(
            merge(
                isInPage
                    ? fetchViewerSettings(requestGraphQL)
                    : observeStorageKey('sync', 'sourcegraphURL').pipe(
                          switchMap(() => fetchViewerSettings(requestGraphQL))
                      ),
                updatedViewerSettings
            ).pipe(
                publishReplay(1),
                refCount()
            ),
            storageSettingsCascade
        ).pipe(
            map(([gqlCascade, storageCascade]) =>
                mergeCascades(
                    gqlToCascade(gqlCascade),
                    gqlCascade.subjects.some(subject => subject.__typename === 'User')
                        ? EMPTY_SETTINGS_CASCADE
                        : storageCascade
                )
            )
        ),
        updateSettings: async (subject, edit) => {
            if (subject === 'Client') {
                // Support storing settings on the client (in the browser extension) so that unauthenticated
                // Sourcegraph viewers can update settings.
                await updateSettings(context, subject, edit, () => editClientSettings(edit))
                return
            }

            try {
                await updateSettings(context, subject, edit, mutateSettings)
            } catch (error) {
                if ('message' in error && /version mismatch/.test(error.message)) {
                    // The user probably edited the settings in another tab, so
                    // try once more.
                    updatedViewerSettings.next(await fetchViewerSettings(requestGraphQL).toPromise())
                    await updateSettings(context, subject, edit, mutateSettings)
                }
            }
            updatedViewerSettings.next(await fetchViewerSettings(requestGraphQL).toPromise())
        },
        requestGraphQL,
        forceUpdateTooltip: () => {
            // TODO(sqs): implement tooltips on the browser extension
        },
        createExtensionHost: () => createExtensionHost(sourcegraphURL),
        getScriptURLForExtension: async bundleURL => {
            if (isInPage) {
                return bundleURL
            }
            // We need to import the extension's JavaScript file (in importScripts in the Web Worker) from a blob:
            // URI, not its original http:/https: URL, because Chrome extensions are not allowed to be published
            // with a CSP that allowlists https://* in script-src (see
            // https://developer.chrome.com/extensions/contentSecurityPolicy#relaxing-remote-script). (Firefox
            // add-ons have an even stricter restriction.)
            const blobURL = await background.createBlobURL(bundleURL)
            return blobURL
        },
        urlToFile: location => {
            if (urlToFile) {
                // Construct URL to file on code host, if possible.
                return urlToFile(sourcegraphURL, location)
            }
            // Otherwise fall back to linking to Sourcegraph (with an absolute URL).
            return `${sourcegraphURL}${toPrettyBlobURL(location)}`
        },
        sourcegraphURL,
        clientApplication: 'other',
        sideloadedExtensionURL: isInPage
            ? new LocalStorageSubject<string | null>('sideloadedExtensionURL', null)
            : new ExtensionStorageSubject('sideloadedExtensionURL', null),
    }
    return context
}
