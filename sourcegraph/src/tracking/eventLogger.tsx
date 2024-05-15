import { matchPath } from 'react-router'
import uuid from 'uuid'
import { currentUser } from '../auth'
import * as GQL from '../backend/graphqlschema'
import { parseBrowserRepoURL } from '../repo'
import { repoRevRoute } from '../routes'
import { getPathExtension } from '../util'
import { browserExtensionMessageReceived, handleQueryEvents, pageViewQueryParameters } from './analyticsUtils'
import { serverAdmin } from './services/serverAdminWrapper'
import { telligent } from './services/telligentWrapper'

const uidKey = 'sourcegraphAnonymousUid'

class EventLogger {
    private hasStrippedQueryParameters = false
    private user?: GQL.IUser | null

    private anonUid?: string

    constructor() {
        browserExtensionMessageReceived.subscribe(isInstalled => {
            telligent.setUserProperty('installed_chrome_extension', 'true')
            this.log('BrowserExtensionConnectedToServer')

            if (localStorage && localStorage.getItem('eventLogDebug') === 'true') {
                console.debug('%cBrowser extension detected, sync completed', 'color: #aaa')
            }
        })

        currentUser.subscribe(
            user => {
                this.user = user
                if (user) {
                    this.updateUser(user)
                    this.log('UserProfileFetched')
                }
            },
            error => {
                /* noop */
            }
        )
    }

    /**
     * Set user-level properties in all external tracking services
     */
    public updateUser(
        user:
            | GQL.IUser
            | {
                  sourcegraphID: number | null
                  username: string | null
                  email: string | null
              }
    ): void {
        this.setUserIds(user.sourcegraphID, user.username)
        if (user.email) {
            this.setUserEmail(user.email)
        }
    }

    /**
     * Set user ID in Telligent tracker script.
     * @param uniqueSourcegraphId Unique Sourcegraph user ID (corresponds to User.ID from backend)
     * @param username Human-readable user identifier, not guaranteed to always stay the same
     */
    public setUserIds(uniqueSourcegraphId: number | null, username: string | null): void {
        if (username) {
            telligent.setUserProperty('username', username)
        }
        if (uniqueSourcegraphId) {
            telligent.setUserProperty('user_id', uniqueSourcegraphId)
        }
    }

    public setUserEmail(primaryEmail: string): void {
        telligent.setUserProperty('email', primaryEmail)
    }

    /**
     * Log a pageview.
     * Page titles should be specific and human-readable in pascal case, e.g. "SearchResults" or "Blob" or "NewOrg"
     */
    public logViewEvent(pageTitle: string, eventProperties?: any, logUserEvent = true): void {
        if (window.context.userAgentIsBot || !pageTitle) {
            return
        }
        pageTitle = `View${pageTitle}`

        const decoratedProps = {
            ...this.decorateEventProperties(eventProperties),
            ...pageViewQueryParameters(window.location.href),
            page_name: pageTitle,
            page_title: pageTitle,
        }
        telligent.track('view', decoratedProps)
        if (logUserEvent) {
            serverAdmin.trackPageView()
        }
        this.logToConsole(pageTitle, decoratedProps)

        // Use flag to ensure URL query params are only stripped once
        if (!this.hasStrippedQueryParameters) {
            handleQueryEvents(window.location.href)
            this.hasStrippedQueryParameters = true
        }
    }

    /**
     * Log a user action or event.
     * Event labels should be specific and follow a ${noun}${verb} structure in pascal case, e.g. "ButtonClicked" or "SignInInitiated"
     */
    public log(eventLabel: string, eventProperties?: any): void {
        if (window.context.userAgentIsBot || !eventLabel) {
            return
        }

        const decoratedProps = {
            ...this.decorateEventProperties(eventProperties),
            eventLabel,
        }
        telligent.track(eventLabel, decoratedProps)
        serverAdmin.trackAction(eventLabel, decoratedProps)
        this.logToConsole(eventLabel, decoratedProps)
    }

    private logToConsole(eventLabel: string, object?: any): void {
        if (localStorage && localStorage.getItem('eventLogDebug') === 'true') {
            console.debug('%cEVENT %s', 'color: #aaa', eventLabel, object)
        }
    }

    private decorateEventProperties(eventProperties: any): any {
        const props = {
            ...eventProperties,
            is_authed: this.user ? 'true' : 'false',
            path_name: window.location && window.location.pathname ? window.location.pathname.slice(1) : '',
        }

        const match = matchPath<{ repoRev?: string; filePath?: string }>(window.location.pathname, {
            path: repoRevRoute.path,
            exact: repoRevRoute.exact,
        })
        if (match) {
            const u = parseBrowserRepoURL(window.location.href)
            props.repo = u.repoPath
            props.rev = u.rev
            if (u.filePath) {
                props.path = u.filePath
                props.language = getPathExtension(u.filePath)
            }
        }
        return props
    }

    /**
     * Access the Telligent unique user ID stored in a cookie on the user's computer. Cookie TTL is 2 years
     * https://sourcegraph.com/github.com/telligent-data/telligent-javascript-tracker@890d6a69b84fc0518a3e848f5469b34817da69fd/-/blob/src/js/tracker.js#L178
     *
     * Only used on Sourcegraph.com, not on self-hosted Sourcegraph instances.
     */
    private getTelligentDuid(): string {
        return telligent.getTelligentDuid() || ''
    }

    /**
     * Generate a new anonymous user ID if one has not yet been set and stored.
     */
    private generateAnonUserID(): string {
        const telID = this.getTelligentDuid()
        if (telID !== null) {
            return telID
        }
        return uuid.v4()
    }

    /**
     * Get the anonymous identifier for this user (used to allow site admins
     * on a Sourcegraph instance to see a count of unique users on a daily,
     * weekly, and monthly basis).
     */
    public getAnonUserID(): string {
        if (this.anonUid) {
            return this.anonUid
        }

        let id = localStorage.getItem(uidKey)
        if (id === null) {
            id = this.generateAnonUserID()
            localStorage.setItem(uidKey, id)
        }
        this.anonUid = id
        return this.anonUid
    }
}

export const eventLogger = new EventLogger()
