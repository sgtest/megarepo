import React, { useCallback, useState } from 'react'
import { useHistory } from 'react-router'

import { Form } from '@sourcegraph/branded/src/components/Form'
import { gql, useMutation } from '@sourcegraph/shared/src/graphql/graphql'
import * as GQL from '@sourcegraph/shared/src/graphql/schema'
import { Container } from '@sourcegraph/wildcard'

import { refreshAuthenticatedUser } from '../../../auth'
import { UpdateUserResult, UpdateUserVariables } from '../../../graphql-operations'
import { eventLogger } from '../../../tracking/eventLogger'

import { UserProfileFormFields, UserProfileFormFieldsValue } from './UserProfileFormFields'

export const UPDATE_USER = gql`
    mutation UpdateUser($user: ID!, $username: String!, $displayName: String, $avatarURL: String) {
        updateUser(user: $user, username: $username, displayName: $displayName, avatarURL: $avatarURL) {
            id
            username
            displayName
            avatarURL
        }
    }
`

interface Props {
    user: Pick<GQL.IUser, 'id' | 'viewerCanChangeUsername'>
    initialValue: UserProfileFormFieldsValue
    after?: React.ReactFragment
}

/**
 * A form to edit a user's profile.
 */
export const EditUserProfileForm: React.FunctionComponent<Props> = ({ user, initialValue, after }) => {
    const history = useHistory()
    const [updateUser, { data, loading, error }] = useMutation<UpdateUserResult, UpdateUserVariables>(UPDATE_USER, {
        onCompleted: ({ updateUser }) => {
            eventLogger.log('UserProfileUpdated')
            history.replace(`/users/${updateUser.username}/settings/profile`)

            // In case the edited user is the current user, immediately reflect the changes in the
            // UI.
            // TODO: Migrate this to use the Apollo cache
            refreshAuthenticatedUser()
                .toPromise()
                .finally(() => {})
        },
        onError: () => eventLogger.log('UpdateUserFailed'),
    })

    const [userFields, setUserFields] = useState<UserProfileFormFieldsValue>(initialValue)
    const onChange = useCallback<React.ComponentProps<typeof UserProfileFormFields>['onChange']>(
        newValue => setUserFields(previous => ({ ...previous, ...newValue })),
        []
    )

    const onSubmit = useCallback<React.FormEventHandler>(
        event => {
            event.preventDefault()
            eventLogger.log('UpdateUserClicked')
            return updateUser({
                variables: {
                    user: user.id,
                    username: userFields.username,
                    displayName: userFields.displayName,
                    avatarURL: userFields.avatarURL,
                },
            })
        },
        [updateUser, user.id, userFields]
    )

    return (
        <Container>
            <Form className="w-100" onSubmit={onSubmit}>
                <UserProfileFormFields
                    value={userFields}
                    onChange={onChange}
                    usernameFieldDisabled={!user.viewerCanChangeUsername}
                    disabled={loading}
                />
                <button
                    type="submit"
                    className="btn btn-primary"
                    disabled={loading}
                    id="test-EditUserProfileForm__save"
                >
                    Save
                </button>
                {error && <div className="mt-3 alert alert-danger">{error.message}</div>}
                {data?.updateUser && (
                    <div className="mt-3 mb-0 alert alert-success test-EditUserProfileForm__success">
                        User profile updated.
                    </div>
                )}
                {after && (
                    <>
                        <hr className="my-4" />
                        {after}
                    </>
                )}
            </Form>
        </Container>
    )
}
