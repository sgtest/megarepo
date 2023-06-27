import { useCallback } from 'react'

import { VSCodeButton } from '@vscode/webview-ui-toolkit/react'
import classNames from 'classnames'

import { AuthStatus, DOTCOM_URL, LOCAL_APP_URL } from '../src/chat/protocol'

import { ConnectApp } from './ConnectApp'
import { ErrorContainer } from './Error'
import { VSCodeWrapper } from './utils/VSCodeApi'

import styles from './Login.module.css'

interface LoginProps {
    authStatus?: AuthStatus
    endpoint: string | null
    isAppInstalled: boolean
    isAppRunning?: boolean
    vscodeAPI: VSCodeWrapper
    callbackScheme?: string
    appOS?: string
    appArch?: string
    onLoginRedirect: (uri: string) => void
}

const APP_DESC = {
    getStarted: 'Cody for VS Code requires the Cody desktop app to enable context fetching for your private code.',
    download: 'Download and run the Cody desktop app to configure your local code graph.',
    connectApp: 'Cody App detected. All that’s left to do is connect VS Code with Cody App.',
    notRunning: 'Cody for VS Code requires the Cody desktop app to enable context fetching for your private code.',
    comingSoon:
        'We’re working on bringing Cody App to your platform. In the meantime, you can try Cody with open source repositories by signing in to Sourcegraph.com.',
}

export const Login: React.FunctionComponent<React.PropsWithChildren<LoginProps>> = ({
    authStatus,
    endpoint,
    vscodeAPI,
    callbackScheme,
    appOS,
    appArch,
    isAppInstalled = false,
    isAppRunning = false,
    onLoginRedirect,
}) => {
    const isOSSupported = appOS === 'darwin' && appArch === 'arm64'

    const onFooterButtonClick = useCallback(
        (title: 'signin' | 'support') => {
            vscodeAPI.postMessage({ command: 'auth', type: title })
        },
        [vscodeAPI]
    )

    const title = isAppInstalled ? (isAppRunning ? 'Connect with Cody App' : 'Cody App Not Running') : 'Get Started'
    const openMsg = !isAppInstalled ? APP_DESC.getStarted : !isAppRunning ? APP_DESC.notRunning : APP_DESC.connectApp

    const AppConnect: React.FunctionComponent = () => (
        <section className={classNames(styles.section, isOSSupported ? styles.codyGradient : styles.greyGradient)}>
            <h2 className={styles.sectionHeader}>{isAppInstalled ? title : 'Get Started'}</h2>
            <p className={styles.openMessage}>{openMsg}</p>
            {!isAppInstalled && <p className={styles.openMessage}>{APP_DESC.download}</p>}
            <ConnectApp
                isAppInstalled={isAppInstalled}
                vscodeAPI={vscodeAPI}
                isOSSupported={isOSSupported}
                appOS={appOS}
                appArch={appArch}
                isAppRunning={isAppRunning}
                callbackScheme={callbackScheme}
            />
            {!isOSSupported && (
                <small>
                    Sorry, {appOS} {appArch} is not yet supported.
                </small>
            )}
        </section>
    )

    const NoAppConnect: React.FunctionComponent = () => (
        <section className={classNames(styles.section, styles.codyGradient)}>
            <h2 className={styles.sectionHeader}>Cody App for {appOS} coming soon</h2>
            <p className={styles.openMessage}>{APP_DESC.comingSoon}</p>
            <VSCodeButton className={styles.button} type="button" onClick={() => onLoginRedirect(DOTCOM_URL.href)}>
                Signin with Sourcegraph.com
            </VSCodeButton>
        </section>
    )

    const isApp = {
        isInstalled: endpoint === LOCAL_APP_URL.href && isAppInstalled,
        isRunning: isAppRunning,
    }
    return (
        <div className={styles.container}>
            {authStatus && <ErrorContainer authStatus={authStatus} isApp={isApp} endpoint={endpoint} />}
            {/* Signin Sections */}
            <div className={styles.sectionsContainer}>
                <AppConnect />
                {!isOSSupported && <NoAppConnect />}
            </div>
            {/* Footer */}
            <footer className={styles.footer}>
                <VSCodeButton className={styles.button} type="button" onClick={() => onFooterButtonClick('signin')}>
                    Other Sign In Options…
                </VSCodeButton>
                <VSCodeButton className={styles.button} type="button" onClick={() => onFooterButtonClick('support')}>
                    Feedback & Support
                </VSCodeButton>
            </footer>
        </div>
    )
}
