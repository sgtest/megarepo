import React, { useEffect, useState } from 'react'

import { BrowserRouter, NavigateFunction, useLocation } from 'react-router-dom'

import { WildcardThemeContext } from '@sourcegraph/wildcard'

interface CodeMirrorContainerProps {
    navigate: NavigateFunction
    onMount?: () => void
    onRender?: () => void
}

/**
 * Creates the necessary context for React components to be rendered inside
 * CodeMirror.
 */
export const CodeMirrorContainer: React.FunctionComponent<React.PropsWithChildren<CodeMirrorContainerProps>> = ({
    navigate,
    onMount,
    onRender,
    children,
}) => {
    useEffect(() => onRender?.())
    // This should only be called once when the component is mounted
    // eslint-disable-next-line react-hooks/exhaustive-deps
    useEffect(() => onMount?.(), [])

    return (
        <WildcardThemeContext.Provider value={{ isBranded: true }}>
            <BrowserRouter>
                {children}
                <SyncInnerRouterWithParent navigate={navigate} />
            </BrowserRouter>
        </WildcardThemeContext.Provider>
    )
}

const SyncInnerRouterWithParent: React.FC<{ navigate: NavigateFunction }> = ({ navigate }) => {
    const initialLocation = useState(useLocation())[0]
    const location = useLocation()
    useEffect(() => {
        if (
            location.hash === initialLocation.hash &&
            location.pathname === initialLocation.pathname &&
            location.search === initialLocation.search
        ) {
            return
        }
        navigate(location)
    }, [location, navigate, initialLocation])
    return null
}
