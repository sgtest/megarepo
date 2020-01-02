import * as React from 'react'
import { Elements, injectStripe, StripeProvider, ReactStripeElements } from 'react-stripe-elements'
import { billingPublishableKey } from '../productSubscriptions/features'

type Props<P> = P & { component: React.ComponentType<P & ReactStripeElements.InjectedStripeProps> }

interface State<P> {
    injectedComponent: React.ComponentType<P>
    stripe: stripe.Stripe | null
}

/**
 * Wraps a React tree (of elements) and injects the Stripe API.
 */
export class StripeWrapper<P extends object> extends React.PureComponent<Props<P>, State<P>> {
    constructor(props: Props<P>) {
        super(props)
        this.state = {
            injectedComponent: injectStripe<P>(props.component),
            stripe: null,
        }
    }

    public componentDidMount(): void {
        const id = 'stripe-script'
        if (document.getElementById(id)) {
            this.setStripeState()
            return // already loaded
        }
        const script = document.createElement('script')
        script.id = id
        script.src = 'https://js.stripe.com/v3/'
        script.async = true
        script.onload = () => this.setStripeState()
        document.body.appendChild(script)
    }

    public componentDidUpdate(prevProps: Props<P>): void {
        if (prevProps.component !== this.props.component) {
            /* eslint react/no-did-update-set-state: warn */
            this.setState({ injectedComponent: injectStripe<P>(this.props.component) })
        }
    }

    public render(): JSX.Element | null {
        if (!this.state.stripe) {
            return null
        }
        const InjectedComponent = this.state.injectedComponent
        return (
            <StripeProvider stripe={this.state.stripe}>
                <Elements>
                    <InjectedComponent {...this.props} />
                </Elements>
            </StripeProvider>
        )
    }

    private setStripeState(): void {
        this.setState({ stripe: (window as any).Stripe(billingPublishableKey) })
    }
}
