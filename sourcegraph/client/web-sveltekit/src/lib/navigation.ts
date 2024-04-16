import { svelteKitRoutes, type SvelteKitRoute } from './routes'

let knownRoutesRegex: RegExp | undefined

function getKnownRoutesRegex(): RegExp {
    if (!knownRoutesRegex) {
        knownRoutesRegex = new RegExp(`(${window.context?.svelteKit?.knownRoutes?.join(')|(')})`)
    }
    return knownRoutesRegex
}

/**
 * Returns whether the SvelteKit app is enabled for the given route ID.
 * If not the caller should trigger a page reload to load the React app.
 * The enabled routes are provided by the server via `window.context`.
 *
 * Callers should pass an actual route ID retrived from SvelteKit not an
 * arbitrary path.
 */
export function isRouteEnabled(pathname: string): boolean {
    if (!pathname) {
        return false
    }
    const enabledRoutes = window.context?.svelteKit?.enabledRoutes ?? []

    let foundRoute: SvelteKitRoute | undefined

    for (const routeIndex of enabledRoutes) {
        const route = svelteKitRoutes.at(routeIndex)
        if (route && route.pattern.test(pathname)) {
            foundRoute = route
            if (!route.isRepoRoot) {
                break
            }
            // If the found route is the repo root we have to keep going
            // to find a more specific route.
        }
    }

    if (foundRoute) {
        if (foundRoute.isRepoRoot) {
            // Check known routes to see if there is a more specific route than the repo root.
            // If yes then we should load the React app (if the more specific route was enabled
            // it would have been found above).
            return !getKnownRoutesRegex().test(pathname)
        }
        return true
    }

    return false
}

/**
 * Helper function to determine whether a route is a repository route.
 * Callers can get the current route ID from the `page` store.
 */
export function isRepoRoute(routeID: string | null): boolean {
    if (!routeID) {
        return false
    }
    return routeID.startsWith('/[...repo=reporev]')
}
