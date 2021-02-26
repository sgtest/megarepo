import { useEffect, useState } from 'react'
import { matchPath, useLocation } from 'react-router'
import { LayoutRouteProps } from '../routes'

/**
 * Match the current pathname against a list of route patterns
 *
 * @param routes List of routes to match against
 *
 * @returns A matching route pattern
 */
export const useRoutesMatch = (routes: readonly LayoutRouteProps<{}>[]): string | undefined => {
    const location = useLocation()
    const [match, setMatch] = useState<string | undefined>('')

    useEffect(() => {
        const newMatch = routes.find(({ path, exact }) => matchPath(location.pathname, { path, exact }))?.path
        setMatch(newMatch)
    }, [location.pathname, routes])

    return match
}
