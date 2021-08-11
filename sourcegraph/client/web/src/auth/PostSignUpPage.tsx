import React, { FunctionComponent, useState, useEffect, useCallback, useRef } from 'react'
import { useLocation, useHistory } from 'react-router'

import { Link } from '@sourcegraph/shared/src/components/Link'
import { LinkOrSpan } from '@sourcegraph/shared/src/components/LinkOrSpan'
import { TelemetryService } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ErrorLike } from '@sourcegraph/shared/src/util/errors'
import { useLocalStorage } from '@sourcegraph/shared/src/util/useLocalStorage'
import { BrandLogo } from '@sourcegraph/web/src/components/branding/BrandLogo'
import { HeroPage } from '@sourcegraph/web/src/components/HeroPage'

import { AuthenticatedUser } from '../auth'
import { PageTitle } from '../components/PageTitle'
import { SourcegraphContext } from '../jscontext'
import { SelectAffiliatedRepos } from '../user/settings/repositories/SelectAffiliatedRepos'
import { UserExternalServicesOrRepositoriesUpdateProps } from '../util'

import { getReturnTo } from './SignInSignUpCommon'
import { Steps, Step, StepList, StepPanels, StepPanel, StepActions } from './Steps'
import { useExternalServices } from './useExternalServices'
import { CodeHostsConnection } from './welcome/CodeHostsConnection'
import { Footer } from './welcome/Footer'
import { StartSearching } from './welcome/StartSearching'

interface PostSignUpPage {
    authenticatedUser: AuthenticatedUser
    context: Pick<SourcegraphContext, 'authProviders'>
    telemetryService: TelemetryService
    onUserExternalServicesOrRepositoriesUpdate: UserExternalServicesOrRepositoriesUpdateProps['onUserExternalServicesOrRepositoriesUpdate']
    setSelectedSearchContextSpec: (spec: string) => void
}

interface Step {
    content: React.ReactElement
    isComplete: () => boolean
    prefetch?: () => void
    onNextButtonClick?: () => Promise<void>
}

export type RepoSelectionMode = 'all' | 'selected' | undefined

const USER_FINISHED_WELCOME_FLOW = 'finished-welcome-flow'

export const PostSignUpPage: FunctionComponent<PostSignUpPage> = ({
    authenticatedUser: user,
    context,
    telemetryService,
    onUserExternalServicesOrRepositoriesUpdate,
    setSelectedSearchContextSpec,
}) => {
    const [didUserFinishWelcomeFlow, setUserFinishedWelcomeFlow] = useLocalStorage(USER_FINISHED_WELCOME_FLOW, false)
    const isOAuthCall = useRef(false)
    const location = useLocation()
    const history = useHistory()

    const goToSearch = (): void => history.push(getReturnTo(location))

    // if the welcome flow was already finished - navigate to search
    if (didUserFinishWelcomeFlow) {
        goToSearch()
    }

    const finishWelcomeFlow = (): void => {
        setUserFinishedWelcomeFlow(true)
        goToSearch()
    }

    const [repoSelectionMode, setRepoSelectionMode] = useState<RepoSelectionMode>()
    const [error, setError] = useState<ErrorLike>()
    const { externalServices, loadingServices, errorServices, refetchExternalServices } = useExternalServices(user.id)

    const hasErrors = error || errorServices

    const beforeUnload = useCallback((): void => {
        // user is not leaving the flow, it's an OAuth page refresh
        if (isOAuthCall.current) {
            return
        }

        setUserFinishedWelcomeFlow(true)
    }, [setUserFinishedWelcomeFlow])

    useEffect(() => {
        if (hasErrors) {
            return
        }

        window.addEventListener('beforeunload', beforeUnload)

        return () => window.removeEventListener('beforeunload', beforeUnload)
    }, [beforeUnload, error, hasErrors])

    const onError = useCallback((error: ErrorLike) => setError(error), [])

    return (
        <>
            <LinkOrSpan to={getReturnTo(location)} className="post-signup-page__logo-link">
                <BrandLogo
                    className="ml-3 mt-3 post-signup-page__logo"
                    isLightTheme={true}
                    variant="symbol"
                    onClick={finishWelcomeFlow}
                />
            </LinkOrSpan>

            <div className="signin-signup-page post-signup-page">
                <PageTitle title="Welcome" />
                <HeroPage
                    lessPadding={true}
                    className="text-left"
                    body={
                        <div className="post-signup-page__container">
                            {hasErrors && (
                                <div className="alert alert-danger mb-4" role="alert">
                                    Sorry, something went wrong. Try refreshing the page or{' '}
                                    <Link to="/search">skip to code search</Link>.
                                </div>
                            )}
                            <h2>Get started with Sourcegraph</h2>
                            <p className="text-muted pb-3">
                                Three quick steps to add your repositories and get searching with Sourcegraph
                            </p>
                            <div className="mt-4 pb-3">
                                <Steps initialStep={1}>
                                    <StepList numeric={true}>
                                        <Step borderColor="purple">Connect with code hosts</Step>
                                        <Step borderColor="blue">Add repositories</Step>
                                        <Step borderColor="orange">Start searching</Step>
                                    </StepList>
                                    <StepPanels>
                                        <StepPanel>
                                            <CodeHostsConnection
                                                user={user}
                                                onNavigation={(called: boolean) => {
                                                    isOAuthCall.current = called
                                                }}
                                                loading={loadingServices}
                                                onError={onError}
                                                externalServices={externalServices}
                                                context={context}
                                                refetch={refetchExternalServices}
                                            />
                                        </StepPanel>
                                        <StepPanel>
                                            <div className="mt-5">
                                                <h3>Add repositories</h3>
                                                <p className="text-muted mb-4">
                                                    Choose repositories you own or collaborate on from your code hosts
                                                    to search with Sourcegraph. We’ll sync and index these repositories
                                                    so you can search your code all in one place.
                                                </p>
                                                <SelectAffiliatedRepos
                                                    authenticatedUser={user}
                                                    onRepoSelectionModeChange={setRepoSelectionMode}
                                                    telemetryService={telemetryService}
                                                    onError={onError}
                                                />
                                            </div>
                                        </StepPanel>
                                        <StepPanel>
                                            <StartSearching
                                                user={user}
                                                repoSelectionMode={repoSelectionMode}
                                                onUserExternalServicesOrRepositoriesUpdate={
                                                    onUserExternalServicesOrRepositoriesUpdate
                                                }
                                                setSelectedSearchContextSpec={setSelectedSearchContextSpec}
                                                onError={onError}
                                            />
                                        </StepPanel>
                                    </StepPanels>
                                    <StepActions>
                                        <Footer onFinish={finishWelcomeFlow} />
                                    </StepActions>
                                </Steps>
                            </div>
                        </div>
                    }
                />
            </div>
        </>
    )
}
