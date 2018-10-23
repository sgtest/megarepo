import React from 'react'
import { Redirect } from 'react-router'
import * as GQL from '../backend/graphqlschema'

type WithOptionalAuthenticatedUser<P extends object> = Pick<P, Exclude<keyof P, 'authenticatedUser'>> & {
    authenticatedUser: GQL.IUser | null
}

/**
 * Wraps a React component and requires an authenticated user. If the viewer is not authenticated, it redirects to
 * the sign-in flow.
 */
export const withAuthenticatedUser = <P extends object & { authenticatedUser: GQL.IUser }>(
    Component: React.ComponentType<P>
): React.ComponentType<WithOptionalAuthenticatedUser<P>> => props => {
    const { authenticatedUser } = props
    // If not logged in, redirect to sign in.
    if (!authenticatedUser) {
        const newUrl = new URL(window.location.href)
        newUrl.pathname = '/sign-in'
        // Return to the current page after sign up/in.
        newUrl.searchParams.set('returnTo', window.location.href)
        return <Redirect to={newUrl.pathname + newUrl.search} />
    }
    return <Component {...props} authenticatedUser={authenticatedUser} />
}
