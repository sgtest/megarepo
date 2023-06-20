import { FC, useCallback, useLayoutEffect } from 'react'

import { appWindow, LogicalSize } from '@tauri-apps/api/window'

import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Theme, ThemeContext, ThemeSetting, useTheme } from '@sourcegraph/shared/src/theme'

import { PageTitle } from '../../../components/PageTitle'
import { SetupStepsContent, SetupStepsRoot, StepConfiguration } from '../../../setup-wizard'

import { AppAllSetSetupStep } from './steps/AppAllSetSetupStep'
import { AppInstallExtensionsSetupStep } from './steps/AppInstallExtensionsSetupStep'
import { AddLocalRepositoriesSetupPage } from './steps/AppLocalRepositoriesSetupStep'
import { AppWelcomeSetupStep } from './steps/AppWelcomeSetupStep'
import { AppEmbeddingsSetupStep } from './steps/embeddings-step/AppEmbeddingsSetupStep'

import styles from './AppSetupWizard.module.scss'

const APP_SETUP_STEPS: StepConfiguration[] = [
    {
        id: 'welcome',
        name: 'Welcome page',
        path: 'welcome',
        component: AppWelcomeSetupStep,
    },
    {
        id: 'local-repositories',
        name: 'Add local repositories',
        path: 'local-repositories',
        component: AddLocalRepositoriesSetupPage,
    },
    {
        id: 'embeddings',
        name: 'Pick repositories for embeddings',
        path: 'embeddings',
        component: AppEmbeddingsSetupStep,
    },
    {
        id: 'install-extensions',
        name: 'Install Sourcegraph extensions',
        path: 'install-extensions',
        component: AppInstallExtensionsSetupStep,
    },
    {
        id: 'all-set',
        name: 'All set',
        path: 'all-set',
        component: AppAllSetSetupStep,
        nextURL: '/',
        onView: () => {
            localStorage.setItem('app.setup.finished', 'true')
        },
        onNext: async () => {
            await appWindow.setResizable(true)
            await appWindow.setSize(new LogicalSize(1024, 768))
        },
    },
]

/**
 * App related setup wizard component, see {@link SetupWizard} component
 * for any other deploy type setup flows.
 */
export const AppSetupWizard: FC<TelemetryProps> = ({ telemetryService }) => {
    const { theme } = useTheme()
    const [activeStepId, setStepId, status] = useTemporarySetting('app-setup.activeStepId')

    const handleStepChange = useCallback(
        (nextStep: StepConfiguration): void => {
            const currentStepIndex = APP_SETUP_STEPS.findIndex(step => step.id === nextStep.id)
            const isLastStep = currentStepIndex === APP_SETUP_STEPS.length - 1

            // Reset the last visited step if you're on the last step in the
            // setup pipeline
            setStepId(!isLastStep ? nextStep.id : '')
        },
        [setStepId]
    )

    // Force light theme when app setup wizard is rendered and return
    // original theme-based theme value when app wizard redirects to the
    // main app
    useLayoutEffect(() => {
        document.documentElement.classList.toggle('theme-light', true)
        document.documentElement.classList.toggle('theme-dark', false)

        return () => {
            const isLightTheme = theme === Theme.Light

            document.documentElement.classList.toggle('theme-light', isLightTheme)
            document.documentElement.classList.toggle('theme-dark', !isLightTheme)
        }
    }, [theme])

    // Local screen size when we are in app setup flow,
    // see last step onNext for window size restore and resize
    // unblock
    useLayoutEffect(() => {
        async function lockSize(): Promise<void> {
            await appWindow.setSize(new LogicalSize(940, 640))
            await appWindow.setResizable(false)
        }

        lockSize().catch(() => {})
    }, [])

    if (status !== 'loaded') {
        return null
    }

    return (
        <ThemeContext.Provider value={{ themeSetting: ThemeSetting.Light }}>
            <PageTitle title="Cody App setup" />

            <SetupStepsRoot
                baseURL="/app-setup/"
                initialStepId={activeStepId}
                steps={APP_SETUP_STEPS}
                onStepChange={handleStepChange}
            >
                <SetupStepsContent telemetryService={telemetryService} className={styles.content} />
            </SetupStepsRoot>
        </ThemeContext.Provider>
    )
}
