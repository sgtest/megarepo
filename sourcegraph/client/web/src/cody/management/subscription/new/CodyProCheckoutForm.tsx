import React, { useState, useEffect } from 'react'

import { EmbeddedCheckoutProvider, EmbeddedCheckout } from '@stripe/react-stripe-js'
import * as stripeJs from '@stripe/stripe-js'
import { useSearchParams } from 'react-router-dom'

import { H3, Text } from '@sourcegraph/wildcard'

/**
 * CodyProCheckoutForm is essentially an iframe that the Stripe Elements library will
 * render an iframe into, that will host a Stripe Checkout-hosted form.
 */
export const CodyProCheckoutForm: React.FunctionComponent<{
    stripeHandle: Promise<stripeJs.Stripe | null>
    customerEmail: string | undefined
}> = ({ stripeHandle, customerEmail }) => {
    const [clientSecret, setClientSecret] = useState('')
    const [errorDetails, setErrorDetails] = useState('')
    const [urlSearchParams] = useSearchParams()

    // Optionally support the "showCouponCodeAtCheckout" URL query parameter, which if present
    // will display a "promotional code" element in the Stripe Checkout UI.
    const showPromoCodeField = urlSearchParams.get('showCouponCodeAtCheckout') !== null

    // Issue an API call to the backend asking it to create a new checkout session.
    // This will update clientSecret/errorDetails asynchronously when the request completes.
    useEffect(() => {
        // useEffect will not accept a Promise, so we call
        // createCheckoutSession and let it run async.
        // (And not `await createCheckoutSession` or `return createCheckoutSession`.)
        void createCheckoutSession('monthly', showPromoCodeField, customerEmail, setClientSecret, setErrorDetails)
    }, [customerEmail, showPromoCodeField, setClientSecret, setErrorDetails])

    const options /* unexported EmbeddedCheckoutProviderProps.options */ = {
        clientSecret,
    }
    return (
        <div>
            {errorDetails && (
                <>
                    <H3>Awe snap!</H3>
                    <Text>There was an error creating the checkout session: {errorDetails}</Text>
                </>
            )}

            {clientSecret && (
                <EmbeddedCheckoutProvider stripe={stripeHandle} options={options}>
                    <EmbeddedCheckout />
                </EmbeddedCheckoutProvider>
            )}
        </div>
    )
}

// createSessionResponse is the API response returned from the SSC backend when
// we ask it to create a new Stripe Checkout Session.
interface createSessionResponse {
    clientSecret: string
}

// createCheckoutSession initiates the API request to the backend to create a Stripe Checkout session.
// Upon completion, the `setClientSecret` or `setErrorDetails` will be called to report the result.
async function createCheckoutSession(
    billingInterval: string,
    showPromoCodeField: boolean,
    customerEmail: string | undefined,
    setClientSecret: (arg: string) => void,
    setErrorDetails: (arg: string) => void
): Promise<void> {
    // e.g. "https://sourcegraph.com"
    const origin = window.location.origin

    try {
        // So the request is kinda made to 2x backends. dotcom's .api/ssc/proxy endpoint will
        // take care of exchanging the Sourcegraph session credentials for a SAMS access token.
        // And then proxy the request onto the SSC backend, which will actually create the
        // checkout session.
        const response = await fetch(`${origin}/.api/ssc/proxy/checkout/session`, {
            // Pass along the "sgs" session cookie to identify the caller.
            credentials: 'same-origin',
            method: 'POST',
            // Object sent to the backend. See `createCheckoutSessionRequest`.
            body: JSON.stringify({
                interval: billingInterval,
                seats: 1,
                customerEmail,
                showPromoCodeField,

                // URL the user is redirected to when the checkout process is complete.
                //
                // BUG: Due to the race conditions between Stripe, the SSC backend,
                // and Sourcegraph.com, immediately loading the Dashboard page isn't
                // going to show the right data reliably. We will need to instead show
                // some intersitular or welcome prompt, to give various things to sync.
                returnUrl: `${origin}/cody/manage?session_id={CHECKOUT_SESSION_ID}`,
            }),
        })

        const responseBody = await response.text()
        if (response.status >= 200 && response.status <= 299) {
            const typedResp = JSON.parse(responseBody) as createSessionResponse
            setClientSecret(typedResp.clientSecret)
        } else {
            // Pass any 4xx or 5xx directly to the user. We expect the
            // server to have properly redcated any sensive information.
            setErrorDetails(responseBody)
        }
    } catch (error) {
        setErrorDetails(`unhandled exception: ${JSON.stringify(error)}`)
    }
}
