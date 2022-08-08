import { EMPTY, Observable, Subject } from 'rxjs'
import { bufferTime, catchError, concatMap, map } from 'rxjs/operators'

import { createAggregateError } from '@sourcegraph/common'
import { gql, dataOrThrowErrors } from '@sourcegraph/http-client'
import { UserEvent, EventSource, Scalars } from '@sourcegraph/shared/src/graphql-operations'

import { requestGraphQL } from '../../backend/graphql'
import {
    LogUserEventResult,
    LogUserEventVariables,
    SetUserEmailVerifiedResult,
    SetUserEmailVerifiedVariables,
    UpdatePasswordResult,
    UpdatePasswordVariables,
    CreatePasswordResult,
    CreatePasswordVariables,
    LogEventsResult,
    LogEventsVariables,
    Event,
} from '../../graphql-operations'
import { eventLogger } from '../../tracking/eventLogger'

export function updatePassword(args: UpdatePasswordVariables): Observable<void> {
    return requestGraphQL<UpdatePasswordResult, UpdatePasswordVariables>(
        gql`
            mutation UpdatePassword($oldPassword: String!, $newPassword: String!) {
                updatePassword(oldPassword: $oldPassword, newPassword: $newPassword) {
                    alwaysNil
                }
            }
        `,
        args
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.updatePassword) {
                eventLogger.log('UpdatePasswordFailed')
                throw createAggregateError(errors)
            }
            eventLogger.log('PasswordUpdated')
        })
    )
}

export function createPassword(args: CreatePasswordVariables): Observable<void> {
    return requestGraphQL<CreatePasswordResult, CreatePasswordVariables>(
        gql`
            mutation CreatePassword($newPassword: String!) {
                createPassword(newPassword: $newPassword) {
                    alwaysNil
                }
            }
        `,
        args
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.createPassword) {
                eventLogger.log('CreatePasswordFailed')
                throw createAggregateError(errors)
            }
            eventLogger.log('PasswordCreated')
        })
    )
}

/**
 * Set the verification state for a user email address.
 *
 * @param user the user's GraphQL ID
 * @param email the email address to edit
 * @param verified the new verification state for the user email
 */
export function setUserEmailVerified(user: Scalars['ID'], email: string, verified: boolean): Observable<void> {
    return requestGraphQL<SetUserEmailVerifiedResult, SetUserEmailVerifiedVariables>(
        gql`
            mutation SetUserEmailVerified($user: ID!, $email: String!, $verified: Boolean!) {
                setUserEmailVerified(user: $user, email: $email, verified: $verified) {
                    alwaysNil
                }
            }
        `,
        { user, email, verified }
    ).pipe(
        map(({ data, errors }) => {
            if (!data || (errors && errors.length > 0)) {
                throw createAggregateError(errors)
            }
        })
    )
}

/**
 * Log a user action (used to allow site admins on a Sourcegraph instance
 * to see a count of unique users on a daily, weekly, and monthly basis).
 *
 * Not used at all for public/sourcegraph.com usage.
 *
 * @deprecated Use logEvent
 */
export function logUserEvent(event: UserEvent): void {
    requestGraphQL<LogUserEventResult, LogUserEventVariables>(
        gql`
            mutation LogUserEvent($event: UserEvent!, $userCookieID: String!) {
                logUserEvent(event: $event, userCookieID: $userCookieID) {
                    alwaysNil
                }
            }
        `,
        { event, userCookieID: eventLogger.getAnonymousUserID() }
    )
        .pipe(
            map(({ data, errors }) => {
                if (!data || (errors && errors.length > 0)) {
                    throw createAggregateError(errors)
                }
                return
            })
        )
        // Event logs are best-effort and non-blocking
        // eslint-disable-next-line rxjs/no-ignored-subscription
        .subscribe()
}

// Log events in batches.
const batchedEvents = new Subject<Event>()

export const logEventsMutation = gql`
    mutation LogEvents($events: [Event!]) {
        logEvents(events: $events) {
            alwaysNil
        }
    }
`

function sendEvents(events: Event[]): Promise<void> {
    return requestGraphQL<LogEventsResult, LogEventsVariables>(logEventsMutation, {
        events,
    })
        .toPromise()
        .then(dataOrThrowErrors)
        .then(() => {})
}

function sendEvent(event: Event): Promise<void> {
    return sendEvents([event])
}

batchedEvents
    .pipe(
        bufferTime(1000),
        concatMap(events => {
            if (events.length > 0) {
                return sendEvents(events)
            }
            return EMPTY
        }),
        catchError(error => {
            console.error('Error logging events:', error)
            return []
        })
    )
    // eslint-disable-next-line rxjs/no-ignored-subscription
    .subscribe()

/**
 * Log a raw user action (used to allow site admins on a Sourcegraph instance
 * to see a count of unique users on a daily, weekly, and monthly basis).
 *
 * When invoked on a non-Sourcegraph.com instance, this data is stored in the
 * instance's database, and not sent to Sourcegraph.com.
 */
export function logEvent(event: string, eventProperties?: unknown, publicArgument?: unknown): void {
    batchedEvents.next(createEvent(event, eventProperties, publicArgument))
}

/**
 * Log a raw user action and return a promise that resolves once the event has
 * been sent. This method will not attempt to batch requests, so it should be
 * used only when low event latency is necessary (e.g., on an external link).
 *
 * See logEvent for additional details.
 */
export function logEventSynchronously(
    event: string,
    eventProperties?: unknown,
    publicArgument?: unknown
): Promise<void> {
    return sendEvent(createEvent(event, eventProperties, publicArgument))
}

function createEvent(event: string, eventProperties?: unknown, publicArgument?: unknown): Event {
    return {
        event,
        userCookieID: eventLogger.getAnonymousUserID(),
        cohortID: eventLogger.getCohortID() || null,
        firstSourceURL: eventLogger.getFirstSourceURL(),
        lastSourceURL: eventLogger.getLastSourceURL(),
        referrer: eventLogger.getReferrer(),
        url: window.location.href,
        source: EventSource.WEB,
        argument: eventProperties ? JSON.stringify(eventProperties) : null,
        publicArgument: publicArgument ? JSON.stringify(publicArgument) : null,
        deviceID: window.context.sourcegraphDotComMode ? eventLogger.getDeviceID() : null,
        eventID: window.context.sourcegraphDotComMode ? eventLogger.getEventID() : null,
        insertID: window.context.sourcegraphDotComMode ? eventLogger.getInsertID() : null,
    }
}
