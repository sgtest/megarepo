import * as H from 'history'
import * as React from 'react'
import { Redirect } from 'react-router'
import * as GQL from '../../../../shared/src/graphql/schema'
import { withAuthenticatedUser } from '../../auth/withAuthenticatedUser'

/**
 * Redirects from /settings to /user/$USERNAME/searches, where $USERNAME is the currently authenticated user's
 * username.
 */
export const RedirectToUserSavedSearches = withAuthenticatedUser(
    ({ authenticatedUser, location }: { authenticatedUser: GQL.IUser; location: H.Location }) => (
        <Redirect
            to={{
                pathname: `/users/${authenticatedUser.username}/searches`,
                search: location.search,
                hash: location.hash,
            }}
        />
    )
)
