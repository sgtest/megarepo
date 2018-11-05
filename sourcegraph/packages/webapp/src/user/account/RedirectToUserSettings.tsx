import * as H from 'history'
import * as React from 'react'
import { Redirect } from 'react-router'
import { userURL } from '..'
import { withAuthenticatedUser } from '../../auth/withAuthenticatedUser'
import * as GQL from '../../backend/graphqlschema'

/**
 * Redirects from /settings to /user/$USERNAME/settings, where $USERNAME is the currently authenticated user's
 * username.
 */
export const RedirectToUserSettings = withAuthenticatedUser(
    ({ authenticatedUser, location }: { authenticatedUser: GQL.IUser; location: H.Location }) => (
        <Redirect
            to={{
                pathname: `${userURL(authenticatedUser.username)}/settings`,
                search: location.search,
                hash: location.hash,
            }}
        />
    )
)
