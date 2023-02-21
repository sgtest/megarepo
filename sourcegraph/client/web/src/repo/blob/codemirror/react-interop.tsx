import React, { useEffect } from 'react'

import { BrowserRouter } from 'react-router-dom'

import { WildcardThemeContext } from '@sourcegraph/wildcard'

interface CodeMirrorContainerProps {
    onMount?: () => void
    onRender?: () => void
}

/**
 * Creates the necessary context for React components to be rendered inside
 * CodeMirror.
 */
export const CodeMirrorContainer: React.FunctionComponent<React.PropsWithChildren<CodeMirrorContainerProps>> = ({
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
            <BrowserRouter>{children}</BrowserRouter>
        </WildcardThemeContext.Provider>
    )
}
