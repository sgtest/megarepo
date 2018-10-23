import { Observable, ReplaySubject } from 'rxjs'
import { catchError, map, mergeMap, tap } from 'rxjs/operators'
import { dataOrThrowErrors, gql, queryGraphQL } from './backend/graphql'
import * as GQL from './backend/graphqlschema'

/**
 * Always represents the latest state of the currently authenticated user.
 *
 * Note that authenticatedUser is not designed to survive across changes in the currently authenticated user. Sign
 * in, sign out, and account changes all require a full-page reload in the browser to take effect.
 */
export const authenticatedUser = new ReplaySubject<GQL.IUser | null>(1)

/**
 * Fetches the current user, orgs, and config state from the remote. Emits no items, completes when done.
 */
export function refreshAuthenticatedUser(): Observable<never> {
    return queryGraphQL(gql`
        query CurrentAuthState {
            currentUser {
                __typename
                id
                sourcegraphID
                username
                avatarURL
                email
                username
                displayName
                siteAdmin
                tags
                url
                organizations {
                    nodes {
                        id
                        name
                    }
                }
                session {
                    canSignOut
                }
                viewerCanAdminister
            }
        }
    `).pipe(
        map(dataOrThrowErrors),
        tap(data => authenticatedUser.next(data.currentUser)),
        catchError(() => {
            authenticatedUser.next(null)
            return []
        }),
        mergeMap(() => [])
    )
}

const initialSiteConfigAuthPublic = window.context.site['auth.public']

/**
 * Whether auth is required to perform any action.
 *
 * If an HTTP request might be triggered by an unauthenticated user on a server with auth.public ==
 * false, the caller must first check authRequired. If authRequired is true, then the component must
 * not initiate the HTTP request. This prevents the browser's devtools console from showing HTTP 401
 * errors, which mislead the user into thinking there is a problem (and make debugging any actual
 * issue much harder).
 */
export const authRequired = authenticatedUser.pipe(map(user => user === null && !initialSiteConfigAuthPublic))

// Populate authenticatedUser.
if (window.context.isAuthenticatedUser) {
    refreshAuthenticatedUser()
        .toPromise()
        .then(() => void 0, err => console.error(err))
} else {
    authenticatedUser.next(null)
}
