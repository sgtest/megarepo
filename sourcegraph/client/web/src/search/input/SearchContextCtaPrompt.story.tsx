import { storiesOf } from '@storybook/react'
import React from 'react'

import { AuthenticatedUser } from '../../auth'
import { WebStory } from '../../components/WebStory'

import { SearchContextCtaPrompt } from './SearchContextCtaPrompt'

const { add } = storiesOf('web/searchContexts/SearchContextCtaPrompt', module)
    .addParameters({
        chromatic: { viewports: [500] },
    })
    .addDecorator(story => (
        <div className="dropdown-menu show" style={{ position: 'static' }}>
            {story()}
        </div>
    ))

export const authUser: AuthenticatedUser = {
    __typename: 'User',
    id: '0',
    email: 'alice@sourcegraph.com',
    username: 'alice',
    avatarURL: null,
    session: { canSignOut: true },
    displayName: null,
    url: '',
    settingsURL: '#',
    siteAdmin: true,
    organizations: {
        nodes: [],
    },
    tags: ['AllowUserExternalServicePublic'],
    viewerCanAdminister: true,
    databaseID: 0,
}

add(
    'not authenticated',
    () => (
        <WebStory>
            {() => <SearchContextCtaPrompt authenticatedUser={null} hasUserAddedExternalServices={false} />}
        </WebStory>
    ),
    {}
)

add(
    'authenticated without added external services',
    () => (
        <WebStory>
            {() => <SearchContextCtaPrompt authenticatedUser={authUser} hasUserAddedExternalServices={false} />}
        </WebStory>
    ),
    {}
)

add(
    'authenticated with added external services',
    () => (
        <WebStory>
            {() => <SearchContextCtaPrompt authenticatedUser={authUser} hasUserAddedExternalServices={true} />}
        </WebStory>
    ),
    {}
)

add(
    'authenticated with private code',
    () => (
        <WebStory>
            {() => (
                <SearchContextCtaPrompt
                    authenticatedUser={{ ...authUser, tags: ['AllowUserExternalServicePrivate'] }}
                    hasUserAddedExternalServices={false}
                />
            )}
        </WebStory>
    ),
    {}
)
