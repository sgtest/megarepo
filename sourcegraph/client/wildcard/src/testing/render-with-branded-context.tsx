import { FC, ReactNode, useEffect } from 'react'

import { RenderResult, render } from '@testing-library/react'
import { InitialEntry } from 'history'
import {
    RouterProvider,
    createMemoryRouter,
    Location,
    useLocation,
    NavigateFunction,
    useNavigate,
    RouteObject,
    Outlet,
} from 'react-router-dom'

import { WildcardThemeContext, WildcardTheme } from '../hooks/useWildcardTheme'

export interface RenderWithBrandedContextResult extends RenderResult {
    locationRef: LocationRef
    navigateRef: NavigateRef
}

interface LocationRef {
    current?: Location
    entries: Location[]
}

interface NavigateRef {
    current?: NavigateFunction
}

interface RenderWithBrandedContextOptions {
    route?: InitialEntry
    path?: string
    /** Required to test redirect URLs. Without the corresponding route react-router doesn't update the location. */
    extraRoutes?: RouteObject[]
}

const wildcardTheme: WildcardTheme = {
    isBranded: true,
}

export function renderWithBrandedContext(
    children: ReactNode,
    options: RenderWithBrandedContextOptions = {}
): RenderWithBrandedContextResult {
    const { route = '/', path = '*', extraRoutes = [] } = options

    const locationRef: LocationRef = {
        current: undefined,
        entries: [],
    }

    const navigateRef: NavigateRef = {
        current: undefined,
    }

    const routes = [
        {
            element: (
                <SyncRouterRefs
                    onLocationChange={location => {
                        locationRef.current = location
                        locationRef.entries.push(location)
                    }}
                    onNavigateChange={navigate => {
                        navigateRef.current = navigate
                    }}
                />
            ),
            children: [
                {
                    path,
                    element: children,
                },
                ...extraRoutes,
            ],
        },
    ] satisfies RouteObject[]

    const router = createMemoryRouter(routes, {
        initialEntries: [route],
    })

    return {
        ...render(
            <WildcardThemeContext.Provider value={wildcardTheme}>
                <RouterProvider router={router} />
            </WildcardThemeContext.Provider>
        ),
        locationRef,
        navigateRef,
    }
}

interface SyncRourterRefProps {
    onLocationChange: (location: Location) => void
    onNavigateChange: (navigate: NavigateFunction) => void
}

const SyncRouterRefs: FC<SyncRourterRefProps> = props => {
    const { onLocationChange, onNavigateChange } = props

    const location = useLocation()
    const navigate = useNavigate()

    useEffect(() => {
        onLocationChange(location)
    }, [onLocationChange, location])

    useEffect(() => {
        onNavigateChange(navigate)
    }, [onNavigateChange, navigate])

    return <Outlet />
}
