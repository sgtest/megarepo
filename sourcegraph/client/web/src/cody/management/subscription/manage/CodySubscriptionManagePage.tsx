import React, { useEffect } from 'react'

import classNames from 'classnames'
import { Navigate } from 'react-router-dom'

import { logger } from '@sourcegraph/common'
import { useQuery } from '@sourcegraph/http-client'
import type { AuthenticatedUser } from '@sourcegraph/shared/src/auth'
import { Alert, Card, LoadingSpinner, PageHeader, Text } from '@sourcegraph/wildcard'

import { withAuthenticatedUser } from '../../../../auth/withAuthenticatedUser'
import { Page } from '../../../../components/Page'
import { PageTitle } from '../../../../components/PageTitle'
import {
    CodySubscriptionPlan,
    type UserCodyPlanResult,
    type UserCodyPlanVariables,
} from '../../../../graphql-operations'
import type { LegacyLayoutRouteContext } from '../../../../LegacyRouteContext'
import { CodyProRoutes } from '../../../codyProRoutes'
import { PageHeaderIcon } from '../../../components/PageHeaderIcon'
import { USER_CODY_PLAN } from '../../../subscription/queries'
import { useCurrentSubscription } from '../../api/react-query/subscriptions'

import { InvoiceHistory } from './InvoiceHistory'
import { PaymentDetails } from './PaymentDetails'
import { SubscriptionDetails } from './SubscriptionDetails'

import styles from './CodySubscriptionManagePage.module.scss'

interface Props extends Pick<LegacyLayoutRouteContext, 'telemetryRecorder'> {
    authenticatedUser: AuthenticatedUser
}

const AuthenticatedCodySubscriptionManagePage: React.FC<Props> = ({ telemetryRecorder }) => {
    const {
        loading: userCodyPlanLoading,
        error: useCodyPlanError,
        data: userCodyPlanData,
    } = useQuery<UserCodyPlanResult, UserCodyPlanVariables>(USER_CODY_PLAN, {})

    useEffect(
        function recordViewEvent() {
            telemetryRecorder.recordEvent('cody.manage-subscription', 'view')
        },
        [telemetryRecorder]
    )

    if (userCodyPlanLoading) {
        return <LoadingSpinner />
    }

    if (useCodyPlanError) {
        logger.error('Failed to fetch Cody subscription data', useCodyPlanError)
        return null
    }

    const subscriptionData = userCodyPlanData?.currentUser?.codySubscription
    if (!subscriptionData) {
        logger.error('Cody subscription data is not available.')
        return null
    }

    // This page only applies to users who have a Cody Pro subscription to manage.
    // Otherwise, direct them to the ./new page to sign up.
    if (subscriptionData.plan !== CodySubscriptionPlan.PRO) {
        return <Navigate to={CodyProRoutes.NewProSubscription} replace={true} />
    }

    return (
        <Page className="d-flex flex-column">
            <PageContent />
        </Page>
    )
}

const PageContent: React.FC = () => {
    const subscriptionQueryResult = useCurrentSubscription()

    if (subscriptionQueryResult.isLoading) {
        return <LoadingSpinner className="mx-auto" />
    }

    if (subscriptionQueryResult.isError) {
        return <Alert variant="danger">Failed to fetch subscription data</Alert>
    }

    const subscription = subscriptionQueryResult?.data
    if (!subscription) {
        return <Alert variant="warning">Subscription data is not available</Alert>
    }

    return (
        <>
            <PageTitle title="Manage Subscription" />
            <PageHeader className="mt-4">
                <PageHeader.Heading as="h2" styleAs="h1" className="mb-4 d-flex align-items-center">
                    <PageHeaderIcon name="cody-logo" className="mr-2" />
                    <Text as="span">Manage subscription</Text>
                </PageHeader.Heading>
            </PageHeader>

            <Card className={classNames('p-4', styles.card)}>
                <SubscriptionDetails subscription={subscription} />

                <hr className={styles.divider} />

                <PaymentDetails subscription={subscription} />
            </Card>

            <Card className={classNames('my-4 p-4', styles.card)}>
                <InvoiceHistory />
            </Card>
        </>
    )
}

export const CodySubscriptionManagePage = withAuthenticatedUser(AuthenticatedCodySubscriptionManagePage)
