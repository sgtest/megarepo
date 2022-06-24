import React, { Suspense, useEffect, useMemo } from 'react'

import { BrowserRouter, Route, RouteComponentProps, Switch, useHistory } from 'react-router-dom'
import { CompatRouter } from 'react-router-dom-v5-compat'

import { createController as createExtensionsController } from '@sourcegraph/shared/src/extensions/controller'
import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'
import { Alert, LoadingSpinner, setLinkComponent, WildcardTheme, WildcardThemeContext } from '@sourcegraph/wildcard'

import '../../SourcegraphWebApp.scss'

import { GlobalContributions } from '../../contributions'
import { createPlatformContext } from '../../platform/context'
import { ThemePreference } from '../../stores/themeState'
import { useTheme } from '../../theme'

import { OpenNewTabAnchorLink } from './OpenNewTabAnchorLink'

import styles from './EmbeddedWebApp.module.scss'

// Since we intend to embed the EmbeddedWebApp component within an iframe,
// we want to open all links in a new tab instead of the current iframe window.
// Otherwise, we would get an error that we tried to access a non-embed route from within the iframe.
setLinkComponent(OpenNewTabAnchorLink)

const WILDCARD_THEME: WildcardTheme = {
    isBranded: true,
}

const EmbeddedNotebookPage = lazyComponent(
    () => import('../../notebooks/notebookPage/EmbeddedNotebookPage'),
    'EmbeddedNotebookPage'
)

const EMPTY_SETTINGS_CASCADE = { final: {}, subjects: [] }

export const EmbeddedWebApp: React.FunctionComponent<React.PropsWithChildren<unknown>> = () => {
    const { enhancedThemePreference, setThemePreference } = useTheme()
    const isLightTheme = enhancedThemePreference === ThemePreference.Light

    useEffect(() => {
        const query = new URLSearchParams(window.location.search)
        const theme = query.get('theme')
        setThemePreference(
            theme === 'dark' ? ThemePreference.Dark : theme === 'light' ? ThemePreference.Light : ThemePreference.System
        )
    }, [setThemePreference])

    useEffect(() => {
        document.documentElement.classList.toggle('theme-light', isLightTheme)
        document.documentElement.classList.toggle('theme-dark', !isLightTheme)
    }, [isLightTheme])

    const platformContext = useMemo(() => createPlatformContext(), [])
    const extensionsController = useMemo(() => createExtensionsController(platformContext), [platformContext])
    const history = useHistory()

    // 🚨 SECURITY: The `EmbeddedWebApp` is intended to be embedded into 3rd party sites where we do not have total control.
    // That is why it is essential to be mindful when adding new routes that may be vulnerable to clickjacking or similar exploits.
    // It is crucial not to embed any components that an attacker could hijack and use to leak personal information (e.g., the sign-in page).
    // The embedded components should be limited to displaying read-only, publicly accessible content.
    //
    // IMPORTANT: Please consult with the security team if you are unsure whether your changes could introduce security exploits.
    return (
        <BrowserRouter>
            <CompatRouter>
                <WildcardThemeContext.Provider value={WILDCARD_THEME}>
                    <div className={styles.body}>
                        <Suspense
                            fallback={
                                <div className="d-flex justify-content-center p-3">
                                    <LoadingSpinner />
                                </div>
                            }
                        >
                            <Switch>
                                <Route
                                    path="/embed/notebooks/:notebookId"
                                    render={(props: RouteComponentProps<{ notebookId: string }>) => (
                                        <EmbeddedNotebookPage
                                            notebookId={props.match.params.notebookId}
                                            searchContextsEnabled={true}
                                            showSearchContext={true}
                                            isSourcegraphDotCom={window.context.sourcegraphDotComMode}
                                            authenticatedUser={null}
                                            isLightTheme={isLightTheme}
                                            settingsCascade={EMPTY_SETTINGS_CASCADE}
                                            platformContext={platformContext}
                                            extensionsController={extensionsController}
                                        />
                                    )}
                                />
                                <Route
                                    path="*"
                                    render={() => (
                                        <Alert variant="danger">
                                            Invalid embedding route, please check the embedding URL.
                                        </Alert>
                                    )}
                                />
                            </Switch>
                        </Suspense>
                        <GlobalContributions
                            extensionsController={extensionsController}
                            platformContext={platformContext}
                            history={history}
                        />
                    </div>
                </WildcardThemeContext.Provider>
            </CompatRouter>
        </BrowserRouter>
    )
}
