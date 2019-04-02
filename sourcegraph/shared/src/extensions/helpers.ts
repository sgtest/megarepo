import { isEqual } from 'lodash'
import { from, Observable, of, throwError } from 'rxjs'
import { catchError, distinctUntilChanged, map, publishReplay, refCount, switchMap } from 'rxjs/operators'
import { gql, graphQLContent } from '../graphql/graphql'
import * as GQL from '../graphql/schema'
import { PlatformContext } from '../platform/context'
import { asError, createAggregateError } from '../util/errors'
import { ConfiguredRegistryExtension, extensionIDsFromSettings, toConfiguredRegistryExtension } from './extension'

/**
 * @returns An observable that emits the list of extensions configured in the viewer's final settings upon
 * subscription and each time it changes.
 */
export function viewerConfiguredExtensions({
    settings,
    queryGraphQL,
}: Pick<PlatformContext, 'settings' | 'queryGraphQL'>): Observable<ConfiguredRegistryExtension[]> {
    return from(settings).pipe(
        map(settings => extensionIDsFromSettings(settings)),
        distinctUntilChanged((a, b) => isEqual(a, b)),
        switchMap(extensionIDs => queryConfiguredRegistryExtensions({ queryGraphQL }, extensionIDs)),
        catchError(error => throwError(asError(error))),
        publishReplay(),
        refCount()
    )
}

/**
 * Query the GraphQL API for registry metadata about the extensions given in {@link extensionIDs}.
 *
 * @returns An observable that emits once with the results.
 */
export function queryConfiguredRegistryExtensions(
    { queryGraphQL }: Pick<PlatformContext, 'queryGraphQL'>,
    extensionIDs: string[]
): Observable<ConfiguredRegistryExtension[]> {
    if (extensionIDs.length === 0) {
        return of([])
    }
    const variables: GQL.IExtensionsOnExtensionRegistryArguments = {
        first: extensionIDs.length,
        prioritizeExtensionIDs: extensionIDs,
    }
    return from(
        queryGraphQL<GQL.IQuery>(
            gql`
                query Extensions($first: Int!, $prioritizeExtensionIDs: [String!]!) {
                    extensionRegistry {
                        extensions(first: $first, prioritizeExtensionIDs: $prioritizeExtensionIDs) {
                            nodes {
                                id
                                extensionID
                                url
                                manifest {
                                    raw
                                }
                                viewerCanAdminister
                            }
                        }
                    }
                }
            `[graphQLContent],
            variables,
            false
        )
    ).pipe(
        map(({ data, errors }) => {
            if (
                !data ||
                !data.extensionRegistry ||
                !data.extensionRegistry.extensions ||
                !data.extensionRegistry.extensions.nodes
            ) {
                throw createAggregateError(errors)
            }
            return data.extensionRegistry.extensions.nodes.map(
                ({ id, extensionID, url, manifest, viewerCanAdminister }) => ({
                    id,
                    extensionID,
                    url,
                    manifest: manifest ? { raw: manifest.raw } : null,
                    viewerCanAdminister,
                })
            )
        }),
        map(registryExtensions => {
            const configuredExtensions: ConfiguredRegistryExtension[] = []
            for (const extensionID of extensionIDs) {
                const registryExtension = registryExtensions.find(x => x.extensionID === extensionID)
                configuredExtensions.push(
                    registryExtension
                        ? toConfiguredRegistryExtension(registryExtension)
                        : { id: extensionID, manifest: null, rawManifest: null, registryExtension: undefined }
                )
            }
            return configuredExtensions
        })
    )
}
