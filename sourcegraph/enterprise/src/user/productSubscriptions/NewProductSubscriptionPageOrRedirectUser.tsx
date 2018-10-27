import * as GQL from '@sourcegraph/webapp/dist/backend/graphqlschema'
import { RedirectToUserPage } from '@sourcegraph/webapp/dist/user/account/RedirectToUserPage'
import React from 'react'
import { RouteComponentProps } from 'react-router'
import { UserSubscriptionsNewProductSubscriptionPage } from './UserSubscriptionsNewProductSubscriptionPage'

interface Props extends RouteComponentProps<{}> {
    authenticatedUser: GQL.IUser | null
    isLightTheme: boolean
}

/**
 * Displays or redirects to the new product subscription page.
 *
 * For authenticated viewers, it redirects to the page under their user account.
 *
 * For unauthenticated viewers, it displays a page that lets them price out a subscription (but requires them to
 * sign in to actually buy it). This friendlier behavior for unauthed viewers (compared to dumping them on a
 * sign-in page) is the reason why this component exists.
 */
export const NewProductSubscriptionPageOrRedirectUser: React.SFC<Props> = props =>
    props.authenticatedUser ? (
        <RedirectToUserPage {...props} />
    ) : (
        <div className="container w-75 mt-4">
            <UserSubscriptionsNewProductSubscriptionPage {...props} user={null} />
        </div>
    )
