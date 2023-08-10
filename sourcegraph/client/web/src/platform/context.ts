import type { ApolloQueryResult, ObservableQuery } from '@apollo/client'
import { map, publishReplay, refCount, shareReplay } from 'rxjs/operators'

import { createAggregateError, asError, logger } from '@sourcegraph/common'
import { fromObservableQueryPromise, getDocumentNode } from '@sourcegraph/http-client'
import { viewerSettingsQuery } from '@sourcegraph/shared/src/backend/settings'
import type { ViewerSettingsResult, ViewerSettingsVariables } from '@sourcegraph/shared/src/graphql-operations'
import type { PlatformContext } from '@sourcegraph/shared/src/platform/context'
import { mutateSettings, updateSettings } from '@sourcegraph/shared/src/settings/edit'
import { gqlToCascade, type SettingsSubject } from '@sourcegraph/shared/src/settings/settings'
import {
    toPrettyBlobURL,
    type RepoFile,
    type UIPositionSpec,
    type ViewStateSpec,
    type RenderModeSpec,
    type UIRangeSpec,
} from '@sourcegraph/shared/src/util/url'

import { getWebGraphQLClient, requestGraphQL } from '../backend/graphql'
import { eventLogger } from '../tracking/eventLogger'

/**
 * Creates the {@link PlatformContext} for the web app.
 */
export function createPlatformContext(): PlatformContext {
    const settingsQueryWatcherPromise = watchViewerSettingsQuery()

    const context: PlatformContext = {
        settings: fromObservableQueryPromise(settingsQueryWatcherPromise).pipe(
            map(mapViewerSettingsResult),
            shareReplay(1),
            map(gqlToCascade),
            publishReplay(1),
            refCount()
        ),
        updateSettings: async (subject, edit) => {
            const settingsQueryWatcher = await settingsQueryWatcherPromise

            // Unauthenticated users can't update settings. (In the browser extension, they can update client
            // settings even when not authenticated. The difference in behavior in the web app vs. browser
            // extension is why this logic lives here and not in shared/.)
            if (!window.context.isAuthenticatedUser) {
                const editDescription =
                    typeof edit === 'string' ? 'edit settings' : 'update setting `' + edit.path.join('.') + '`'
                const url = new URL(window.context.externalURL)
                throw new Error(
                    `Unable to ${editDescription} because you are not signed in.` +
                        '\n\n' +
                        `[**Sign into Sourcegraph${
                            url.hostname === 'sourcegraph.com' ? '' : ` on ${url.host}`
                        }**](${`${url.href.replace(/\/$/, '')}/sign-in`})`
                )
            }

            try {
                await updateSettings(context, subject, edit, mutateSettings)
            } catch (error) {
                if (asError(error).message.includes('version mismatch')) {
                    // The user probably edited the settings in another tab, so
                    // try once more.
                    await settingsQueryWatcher.refetch()
                    await updateSettings(context, subject, edit, mutateSettings)
                } else {
                    throw error
                }
            }

            // The error will be emitted to consumers from the `context.settings` observable.
            await settingsQueryWatcher.refetch().catch(error => logger.error(error))
        },
        getGraphQLClient: getWebGraphQLClient,
        requestGraphQL: ({ request, variables }) => requestGraphQL(request, variables),
        createExtensionHost: () => {
            throw new Error('extensions are no longer supported in the web app')
        },
        urlToFile: toPrettyWebBlobURL,
        sourcegraphURL: window.context.externalURL,
        clientApplication: 'sourcegraph',
        telemetryService: eventLogger,
    }

    return context
}

function toPrettyWebBlobURL(
    context: RepoFile &
        Partial<UIPositionSpec> &
        Partial<ViewStateSpec> &
        Partial<UIRangeSpec> &
        Partial<RenderModeSpec>
): string {
    return toPrettyBlobURL(context)
}

function mapViewerSettingsResult({ data, errors }: ApolloQueryResult<ViewerSettingsResult>): {
    subjects: SettingsSubject[]
} {
    if (!data?.viewerSettings) {
        throw createAggregateError(errors)
    }

    return data.viewerSettings
}

/**
 * Creates Apollo query watcher for the viewer's settings. Watcher is used instead of the one-time query because we
 * want to use cached response if it's available. Callers should use settingsRefreshes#next instead of calling
 * this function, to ensure that the result is propagated consistently throughout the app instead of only being
 * returned to the caller.
 */
async function watchViewerSettingsQuery(): Promise<ObservableQuery<ViewerSettingsResult, ViewerSettingsVariables>> {
    const graphQLClient = await getWebGraphQLClient()

    return graphQLClient.watchQuery<ViewerSettingsResult, ViewerSettingsVariables>({
        query: getDocumentNode(viewerSettingsQuery),
    })
}
