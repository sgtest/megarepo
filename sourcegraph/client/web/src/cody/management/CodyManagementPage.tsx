import React, { useCallback, useEffect } from 'react'

import { mdiCreditCardOutline } from '@mdi/js'
import classNames from 'classnames'
import { useNavigate } from 'react-router-dom'

import { useQuery } from '@sourcegraph/http-client'
import type { TelemetryV2Props } from '@sourcegraph/shared/src/telemetry'
import { ButtonLink, H1, H2, Icon, Link, PageHeader, Text, useSearchParameters } from '@sourcegraph/wildcard'

import type { AuthenticatedUser } from '../../auth'
import { Page } from '../../components/Page'
import { PageTitle } from '../../components/PageTitle'
import {
    type UserCodyPlanResult,
    type UserCodyPlanVariables,
    type UserCodyUsageResult,
    type UserCodyUsageVariables,
    CodySubscriptionPlan,
} from '../../graphql-operations'
import { CodyProIcon, DashboardIcon } from '../components/CodyIcon'
import { isCodyEnabled } from '../isCodyEnabled'
import { CodyOnboarding, type IEditor } from '../onboarding/CodyOnboarding'
import { useCodyPaymentsUrl } from '../subscription/CodySubscriptionPage'
import { USER_CODY_PLAN, USER_CODY_USAGE } from '../subscription/queries'

import { SubscriptionStats } from './SubscriptionStats'
import { UseCodyInEditorSection } from './UseCodyInEditorSection'

import styles from './CodyManagementPage.module.scss'

interface CodyManagementPageProps extends TelemetryV2Props {
    isSourcegraphDotCom: boolean
    authenticatedUser: AuthenticatedUser | null
}

export enum EditorStep {
    SetupInstructions = 0,
    CodyFeatures = 1,
}

export const CodyManagementPage: React.FunctionComponent<CodyManagementPageProps> = ({
    isSourcegraphDotCom,
    authenticatedUser,
    telemetryRecorder,
}) => {
    const parameters = useSearchParameters()

    const utm_source = parameters.get('utm_source')

    useEffect(() => {
        telemetryRecorder.recordEvent('cody.management', 'view')
    }, [utm_source, telemetryRecorder])

    const { data, error: dataError } = useQuery<UserCodyPlanResult, UserCodyPlanVariables>(USER_CODY_PLAN, {})

    const { data: usageData, error: usageDateError } = useQuery<UserCodyUsageResult, UserCodyUsageVariables>(
        USER_CODY_USAGE,
        {}
    )

    const [selectedEditor, setSelectedEditor] = React.useState<IEditor | null>(null)
    const [selectedEditorStep, setSelectedEditorStep] = React.useState<EditorStep | null>(null)

    const subscription = data?.currentUser?.codySubscription

    const codyPaymentsUrl = useCodyPaymentsUrl()
    const manageSubscriptionRedirectURL = `${codyPaymentsUrl}/cody/subscription`

    const navigate = useNavigate()

    useEffect(() => {
        if (!!data && !data?.currentUser) {
            navigate('/sign-in?returnTo=/cody/manage')
        }
    }, [data, navigate])

    const onClickUpgradeToProCTA = useCallback(() => {
        telemetryRecorder.recordEvent('cody.management.upgradeToProCTA', 'click')
    }, [telemetryRecorder])

    if (dataError || usageDateError) {
        throw dataError || usageDateError
    }

    if (!isCodyEnabled() || !isSourcegraphDotCom || !subscription) {
        return null
    }

    const isUserOnProTier = subscription.plan === CodySubscriptionPlan.PRO

    return (
        <>
            <Page className={classNames('d-flex flex-column')}>
                <PageTitle title="Dashboard" />
                <PageHeader className="mb-4 mt-4">
                    <PageHeader.Heading as="h2" styleAs="h1">
                        <div className="d-inline-flex align-items-center">
                            <DashboardIcon className="mr-2" /> Dashboard
                        </div>
                    </PageHeader.Heading>
                </PageHeader>

                {!isUserOnProTier && <UpgradeToProBanner onClick={onClickUpgradeToProCTA} />}

                <div className={classNames('p-4 border bg-1 mt-4', styles.container)}>
                    <div className="d-flex justify-content-between align-items-center border-bottom pb-3">
                        <div>
                            <H2>My subscription</H2>
                            <Text className="text-muted mb-0">
                                {isUserOnProTier ? (
                                    'You are on the Pro tier.'
                                ) : (
                                    <span>
                                        You are on the Free tier.{' '}
                                        <Link to="/cody/subscription">Upgrade to the Pro tier.</Link>
                                    </span>
                                )}
                            </Text>
                        </div>
                        {isUserOnProTier && (
                            <div>
                                <ButtonLink
                                    variant="primary"
                                    size="sm"
                                    href={manageSubscriptionRedirectURL}
                                    onClick={event => {
                                        event.preventDefault()
                                        telemetryRecorder.recordEvent('cody.manageSubscription', 'click')
                                        window.location.href = manageSubscriptionRedirectURL
                                    }}
                                >
                                    <Icon svgPath={mdiCreditCardOutline} className="mr-1" aria-hidden={true} />
                                    Manage subscription
                                </ButtonLink>
                            </div>
                        )}
                    </div>
                    <SubscriptionStats {...{ subscription, usageData }} />
                </div>

                <UseCodyInEditorSection
                    {...{
                        selectedEditor,
                        setSelectedEditor,
                        selectedEditorStep,
                        setSelectedEditorStep,
                        isUserOnProTier,
                        telemetryRecorder,
                    }}
                />
            </Page>
            <CodyOnboarding authenticatedUser={authenticatedUser} telemetryRecorder={telemetryRecorder} />
        </>
    )
}

const UpgradeToProBanner: React.FunctionComponent<{
    onClick: () => void
}> = ({ onClick }) => (
    <div className={classNames('d-flex justify-content-between align-items-center p-4', styles.upgradeToProBanner)}>
        <div>
            <H1>
                Become limitless with
                <CodyProIcon className="ml-1" />
            </H1>
            <ul className="pl-4 mb-0">
                <li>Unlimited autocompletions</li>
                <li>Unlimited chat messages</li>
            </ul>
        </div>
        <div>
            <ButtonLink to="/cody/subscription" variant="primary" size="sm" onClick={onClick}>
                Upgrade
            </ButtonLink>
        </div>
    </div>
)
