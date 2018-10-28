import H from 'history'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Observable, Subject, Subscription } from 'rxjs'
import { catchError, map, mapTo, startWith, switchMap, tap } from 'rxjs/operators'
import { gql, mutateGraphQL } from '../../../../src/backend/graphql'
import * as GQL from '../../../../src/backend/graphqlschema'
import { HeroPage } from '../../../../src/components/HeroPage'
import { PageTitle } from '../../../../src/components/PageTitle'
import { eventLogger } from '../../../../src/tracking/eventLogger'
import { asError, createAggregateError, ErrorLike } from '../../../../src/util/errors'
import { BackToAllSubscriptionsLink } from './BackToAllSubscriptionsLink'
import { ProductSubscriptionForm, ProductSubscriptionFormData } from './ProductSubscriptionForm'

interface Props extends RouteComponentProps<{}> {
    /**
     * The user who will own the new subscrption when created, or null when there is no authenticated user and this
     * page is accessed at /user/subscriptions/new.
     */
    user: GQL.IUser | null

    isLightTheme: boolean
}

const LOADING: 'loading' = 'loading'

interface State {
    /**
     * The result of creating the paid product subscription: null when complete or not started yet,
     * loading, or an error.
     */
    creationOrError: null | typeof LOADING | ErrorLike
}

/**
 * Displays a form and payment flow to purchase a product subscription.
 *
 * This page is visible to both authenticated and unauthenticated users. Unauthenticated users may view it at
 * /user/subscriptions/new and are allowed to price out a subscription, but they must sign in to buy the
 * subscription.
 */
export class UserSubscriptionsNewProductSubscriptionPage extends React.Component<Props, State> {
    public state: State = { creationOrError: null }

    private submits = new Subject<GQL.ICreatePaidProductSubscriptionOnDotcomMutationArguments>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        eventLogger.logViewEvent('UserSubscriptionsNewProductSubscription')
        this.subscriptions.add(
            this.submits
                .pipe(
                    switchMap(args =>
                        createPaidProductSubscription(args).pipe(
                            tap(({ productSubscription }) => {
                                // Redirect to new subscription upon success.
                                this.props.history.push(productSubscription.url)
                            }),
                            mapTo(null),
                            catchError(err => [asError(err)]),
                            startWith(LOADING),
                            map(c => ({ creationOrError: c }))
                        )
                    )
                )
                .subscribe(stateUpdate => this.setState(stateUpdate))
        )
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        if (this.props.user && !this.props.user.viewerCanAdminister) {
            return <HeroPage icon={AlertCircleIcon} title="Not authorized" />
        }

        return (
            <div className="user-subscriptions-new-product-subscription-page">
                <PageTitle title="New product subscription" />
                {this.props.user && <BackToAllSubscriptionsLink user={this.props.user} />}
                <h2>New subscription</h2>
                <ProductSubscriptionForm
                    accountID={this.props.user ? this.props.user.id : null}
                    subscriptionID={null}
                    initialValue={parseProductSubscriptionInputFromLocation(this.props.location) || undefined}
                    isLightTheme={this.props.isLightTheme}
                    onSubmit={this.onSubmit}
                    submissionState={this.state.creationOrError}
                    primaryButtonText="Buy subscription"
                    afterPrimaryButton={
                        <small className="form-text text-muted">
                            Your license key will be available immediately after payment.
                        </small>
                    }
                />
            </div>
        )
    }

    private onSubmit = (args: ProductSubscriptionFormData) => {
        this.submits.next(args)
    }
}

/**
 * Parses product subscription input from the URL hash.
 *
 * Inverse of {@link productSubscriptionInputForLocationHash}.
 */
export function parseProductSubscriptionInputFromLocation(location: H.Location): GQL.IProductSubscriptionInput | null {
    if (location.hash) {
        const params = new URLSearchParams(location.hash.slice('#'.length))
        const billingPlanID = params.get('plan')
        const userCount = parseInt(params.get('userCount') || '0', 10)
        if (billingPlanID && userCount) {
            return { billingPlanID, userCount }
        }
    }
    return null
}

/**
 * Generates the URL hash value to represent the product subscription input.
 *
 * Inverse of {@link parseProductSubscriptionInputFromLocation}.
 */
export function productSubscriptionInputForLocationHash(value: GQL.IProductSubscriptionInput | null): string {
    if (value === null) {
        return ''
    }
    const params = new URLSearchParams()
    params.set('plan', value.billingPlanID)
    params.set('userCount', value.userCount.toString())
    return '#' + params.toString()
}

function createPaidProductSubscription(
    args: GQL.ICreatePaidProductSubscriptionOnDotcomMutationArguments
): Observable<GQL.ICreatePaidProductSubscriptionResult> {
    return mutateGraphQL(
        gql`
            mutation CreatePaidProductSubscription(
                $accountID: ID!
                $productSubscription: ProductSubscriptionInput!
                $paymentToken: String!
            ) {
                dotcom {
                    createPaidProductSubscription(
                        accountID: $accountID
                        productSubscription: $productSubscription
                        paymentToken: $paymentToken
                    ) {
                        productSubscription {
                            id
                            name
                            url
                        }
                    }
                }
            }
        `,
        args
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.dotcom || !data.dotcom.createPaidProductSubscription || (errors && errors.length > 0)) {
                throw createAggregateError(errors)
            }
            return data.dotcom.createPaidProductSubscription
        })
    )
}
