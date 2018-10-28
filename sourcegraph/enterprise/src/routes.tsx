import { LayoutRouteProps, routes } from '../../src/routes'
import React from 'react'
import { NewProductSubscriptionPageOrRedirectUser } from './user/productSubscriptions/NewProductSubscriptionPageOrRedirectUser'

export const enterpriseRoutes: ReadonlyArray<LayoutRouteProps> = [
    {
        // Allow unauthenticated viewers to view the "new subscription" page to price out a subscription (instead
        // of just dumping them on a sign-in page).
        path: '/user/subscriptions/new',
        exact: true,
        render: props => <NewProductSubscriptionPageOrRedirectUser {...props} />,
    },
    ...routes,
]
