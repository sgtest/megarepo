import * as H from 'history'
import ArrowLeftIcon from 'mdi-react/ArrowLeftIcon'
import React, { useEffect, useMemo, useCallback } from 'react'
import { RouteComponentProps } from 'react-router'
import { Link } from 'react-router-dom'
import { Observable, throwError } from 'rxjs'
import { catchError, map, mapTo, startWith, switchMap, tap } from 'rxjs/operators'

import { asError, createAggregateError, isErrorLike } from '@sourcegraph/common'
import { gql } from '@sourcegraph/http-client'
import * as GQL from '@sourcegraph/shared/src/schema'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { LoadingSpinner, useEventObservable, useObservable, Button } from '@sourcegraph/wildcard'

import { mutateGraphQL, queryGraphQL } from '../../../backend/graphql'
import { ErrorAlert } from '../../../components/alerts'
import { PageTitle } from '../../../components/PageTitle'
import { eventLogger } from '../../../tracking/eventLogger'

import { ProductSubscriptionForm, ProductSubscriptionFormData } from './ProductSubscriptionForm'

interface Props extends RouteComponentProps<{ subscriptionUUID: string }>, ThemeProps {
    user: Pick<GQL.IUser, 'id'>

    /** For mocking in tests only. */
    _queryProductSubscription?: typeof queryProductSubscription
    history: H.History
}

type ProductSubscription = Pick<GQL.IProductSubscription, 'id' | 'name' | 'invoiceItem' | 'url'>

const LOADING = 'loading' as const

/**
 * Displays a page for editing a product subscription in the user subscriptions area.
 */
export const UserSubscriptionsEditProductSubscriptionPage: React.FunctionComponent<Props> = ({
    user,
    match: {
        params: { subscriptionUUID },
    },
    history,
    isLightTheme,
    _queryProductSubscription = queryProductSubscription,
}) => {
    useEffect(() => eventLogger.logViewEvent('UserSubscriptionsEditProductSubscription'), [])

    /**
     * The product subscription, or loading, or an error.
     */
    const productSubscription =
        useObservable(
            useMemo(
                () =>
                    _queryProductSubscription(subscriptionUUID).pipe(
                        catchError(error => [asError(error)]),
                        startWith(LOADING)
                    ),
                [_queryProductSubscription, subscriptionUUID]
            )
        ) || LOADING

    /**
     * The result of updating the paid product subscription: undefined when complete or not started
     * yet, loading, or an error.
     */
    const [nextUpdate, update] = useEventObservable(
        useCallback(
            (updates: Observable<ProductSubscriptionFormData>) =>
                updates.pipe(
                    switchMap(args => {
                        const subscriptionID =
                            productSubscription !== LOADING && !isErrorLike(productSubscription)
                                ? productSubscription.id
                                : null
                        if (subscriptionID === null) {
                            return throwError(new Error('no product subscription'))
                        }
                        return updatePaidProductSubscription({
                            update: args.productSubscription,
                            subscriptionID,
                            paymentToken: args.paymentToken,
                        }).pipe(
                            tap(({ productSubscription }) => {
                                // Redirect back to subscription upon success.
                                history.push(productSubscription.url)
                            }),
                            mapTo(undefined),
                            startWith(LOADING)
                        )
                    }),
                    catchError(error => [asError(error)])
                ),
            [history, productSubscription]
        )
    )

    return (
        <div className="user-subscriptions-edit-product-subscription-page">
            <PageTitle title="Edit subscription" />
            {productSubscription === LOADING ? (
                <LoadingSpinner />
            ) : isErrorLike(productSubscription) ? (
                <ErrorAlert className="my-2" error={productSubscription} />
            ) : (
                <>
                    <Button to={productSubscription.url} className="mb-3" variant="link" size="sm" as={Link}>
                        <ArrowLeftIcon className="icon-inline" /> Subscription
                    </Button>
                    <h2>Upgrade or change subscription {productSubscription.name}</h2>
                    <ProductSubscriptionForm
                        accountID={user.id}
                        subscriptionID={productSubscription.id}
                        isLightTheme={isLightTheme}
                        onSubmit={nextUpdate}
                        submissionState={update}
                        initialValue={
                            productSubscription.invoiceItem
                                ? {
                                      billingPlanID: productSubscription.invoiceItem.plan.billingPlanID,
                                      userCount: productSubscription.invoiceItem.userCount,
                                  }
                                : undefined
                        }
                        primaryButtonText="Upgrade subscription"
                        afterPrimaryButton={
                            <small className="form-text text-muted">
                                An upgraded license key will be available immediately.
                            </small>
                        }
                        history={history}
                    />
                </>
            )}
        </div>
    )
}

function queryProductSubscription(uuid: string): Observable<ProductSubscription> {
    return queryGraphQL(
        gql`
            query ProductSubscriptionOnEditPage($uuid: String!) {
                dotcom {
                    productSubscription(uuid: $uuid) {
                        ...ProductSubscriptionFieldsOnEditPage
                    }
                }
            }

            fragment ProductSubscriptionFieldsOnEditPage on ProductSubscription {
                id
                name
                invoiceItem {
                    plan {
                        billingPlanID
                    }
                    userCount
                    expiresAt
                }
                url
            }
        `,
        { uuid }
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.dotcom || !data.dotcom.productSubscription || (errors && errors.length > 0)) {
                throw createAggregateError(errors)
            }
            return data.dotcom.productSubscription
        })
    )
}

function updatePaidProductSubscription(
    args: GQL.IUpdatePaidProductSubscriptionOnDotcomMutationArguments
): Observable<GQL.IUpdatePaidProductSubscriptionResult> {
    return mutateGraphQL(
        gql`
            mutation UpdatePaidProductSubscription(
                $subscriptionID: ID!
                $update: ProductSubscriptionInput!
                $paymentToken: String
            ) {
                dotcom {
                    updatePaidProductSubscription(
                        subscriptionID: $subscriptionID
                        update: $update
                        paymentToken: $paymentToken
                    ) {
                        productSubscription {
                            url
                        }
                    }
                }
            }
        `,
        args
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.dotcom || !data.dotcom.updatePaidProductSubscription || (errors && errors.length > 0)) {
                throw createAggregateError(errors)
            }
            return data.dotcom.updatePaidProductSubscription
        })
    )
}
