import cookies, { CookieAttributes } from 'js-cookie'
import * as uuid from 'uuid'

import { TelemetryService } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { UTMMarker } from '@sourcegraph/shared/src/tracking/utm'

import { browserExtensionMessageReceived } from './analyticsUtils'
import { serverAdmin } from './services/serverAdminWrapper'
import { getPreviousMonday, redactSensitiveInfoFromAppURL, stripURLParameters } from './util'

export const ANONYMOUS_USER_ID_KEY = 'sourcegraphAnonymousUid'
export const COHORT_ID_KEY = 'sourcegraphCohortId'
export const FIRST_SOURCE_URL_KEY = 'sourcegraphSourceUrl'
export const LAST_SOURCE_URL_KEY = 'sourcegraphRecentSourceUrl'
export const DEVICE_ID_KEY = 'sourcegraphDeviceId'

export class EventLogger implements TelemetryService {
    private hasStrippedQueryParameters = false

    private anonymousUserID = ''
    private cohortID?: string
    private firstSourceURL?: string
    private lastSourceURL?: string
    private deviceID = ''
    private eventID = 0
    private listeners: Set<(eventName: string) => void> = new Set()

    private readonly cookieSettings: CookieAttributes = {
        // 365 days expiry, but renewed on activity.
        expires: 365,
        // Enforce HTTPS
        secure: true,
        // We only read the cookie with JS so we don't need to send it cross-site nor on initial page requests.
        // However, we do need it on page redirects when users sign up via OAuth, hence using the Lax policy.
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie/SameSite
        sameSite: 'Lax',
        // Specify the Domain attribute to ensure subdomains (about.sourcegraph.com) can receive this cookie.
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Cookies#define_where_cookies_are_sent
        domain: location.hostname,
    }

    constructor() {
        // EventLogger is never teared down
        // eslint-disable-next-line rxjs/no-ignored-subscription
        browserExtensionMessageReceived.subscribe(({ platform, version }) => {
            const args = { platform, version }
            this.log('BrowserExtensionConnectedToServer', args, args)

            if (localStorage && localStorage.getItem('eventLogDebug') === 'true') {
                console.debug('%cBrowser extension detected, sync completed', 'color: #aaa')
            }
        })

        this.initializeLogParameters()
    }

    /**
     * Log a pageview.
     * Page titles should be specific and human-readable in pascal case, e.g. "SearchResults" or "Blob" or "NewOrg"
     */
    public logViewEvent(pageTitle: string, eventProperties?: any, logAsActiveUser = true): void {
        if (window.context?.userAgentIsBot || !pageTitle) {
            return
        }
        pageTitle = `View${pageTitle}`

        const props = pageViewQueryParameters(window.location.href)
        serverAdmin.trackPageView(pageTitle, logAsActiveUser, eventProperties)
        this.logToConsole(pageTitle, props)

        // Use flag to ensure URL query params are only stripped once
        if (!this.hasStrippedQueryParameters) {
            handleQueryEvents(window.location.href)
            this.hasStrippedQueryParameters = true
        }
    }

    /**
     * Log a user action or event.
     * Event labels should be specific and follow a ${noun}${verb} structure in pascal case, e.g. "ButtonClicked" or "SignInInitiated"
     *
     * @param eventLabel: the event name.
     * @param eventProperties: event properties. These get logged to our database, but do not get
     * sent to our analytics systems. This may contain private info such as repository names or search queries.
     * @param publicArgument: event properties that include only public information. Do NOT
     * include any private information, such as full URLs that may contain private repo names or
     * search queries. The contents of this parameter are sent to our analytics systems.
     */
    public log(eventLabel: string, eventProperties?: any, publicArgument?: any): void {
        for (const listener of this.listeners) {
            listener(eventLabel)
        }
        if (window.context?.userAgentIsBot || !eventLabel) {
            return
        }
        serverAdmin.trackAction(eventLabel, eventProperties, publicArgument)
        this.logToConsole(eventLabel, eventProperties, publicArgument)
    }

    private logToConsole(eventLabel: string, eventProperties?: any, publicArgument?: any): void {
        if (localStorage && localStorage.getItem('eventLogDebug') === 'true') {
            console.debug('%cEVENT %s', 'color: #aaa', eventLabel, eventProperties, publicArgument)
        }
    }

    /**
     * Get the anonymous identifier for this user (used to allow site admins
     * on a Sourcegraph instance to see a count of unique users on a daily,
     * weekly, and monthly basis).
     */
    public getAnonymousUserID(): string {
        return this.anonymousUserID
    }

    /**
     * The cohort ID is generated when the anonymous user ID is generated.
     * Users that have visited before the introduction of cohort IDs will not have one.
     */
    public getCohortID(): string | undefined {
        return this.cohortID
    }

    public getFirstSourceURL(): string {
        const firstSourceURL = this.firstSourceURL || cookies.get(FIRST_SOURCE_URL_KEY) || location.href

        const redactedURL = redactSensitiveInfoFromAppURL(firstSourceURL)

        // Use cookies instead of localStorage so that the ID can be shared with subdomains (about.sourcegraph.com).
        // Always set to renew expiry and migrate from localStorage
        cookies.set(FIRST_SOURCE_URL_KEY, redactedURL, this.cookieSettings)

        this.firstSourceURL = firstSourceURL
        return firstSourceURL
    }

    public getLastSourceURL(): string {
        // The cookie value gets overwritten each time a user visits a *.sourcegraph.com property. This code
        // lives in Google Tag Manager.
        const lastSourceURL = this.lastSourceURL || cookies.get(LAST_SOURCE_URL_KEY) || location.href

        const redactedURL = redactSensitiveInfoFromAppURL(lastSourceURL)

        // Use cookies instead of localStorage so that the ID can be shared with subdomains (about.sourcegraph.com).
        // Always set to renew expiry and migrate from localStorage
        cookies.set(LAST_SOURCE_URL_KEY, redactedURL, this.cookieSettings)

        this.lastSourceURL = lastSourceURL
        return lastSourceURL
    }

    // Device ID is a require field for Amplitude events.
    // https://developers.amplitude.com/docs/http-api-v2
    public getDeviceID(): string {
        return this.deviceID
    }

    // Insert ID is used to deduplicate events in Amplitude.
    // https://developers.amplitude.com/docs/http-api-v2#optional-keys
    public getInsertID(): string {
        return uuid.v4()
    }

    // Event ID is used to deduplicate events in Amplitude.
    // This is used in the case that multiple events with the same userID and timestamp
    // are sent. https://developers.amplitude.com/docs/http-api-v2#optional-keys
    public getEventID(): number {
        this.eventID++
        return this.eventID
    }

    public getReferrer(): string {
        const referrer = document.referrer
        try {
            // 🚨 SECURITY: If the referrer is a valid Sourcegraph.com URL,
            // only send the hostname instead of the whole URL to avoid
            // leaking private repository names and files into our data.
            const url = new URL(referrer)
            if (url.hostname === 'sourcegraph.com') {
                return 'sourcegraph.com'
            }
            return referrer
        } catch {
            return ''
        }
    }

    /**
     * Gets the anonymous user ID and cohort ID of the user from cookies.
     * If user doesn't have an anonymous user ID yet, a new one is generated, along with
     * a cohort ID of the week the user first visited.
     *
     * If the user already has an anonymous user ID before the introduction of cohort IDs,
     * the user will not haved a cohort ID.
     *
     * If user had an anonymous user ID in localStorage, it will be migrated to cookies.
     */
    private initializeLogParameters(): void {
        let anonymousUserID = cookies.get(ANONYMOUS_USER_ID_KEY) || localStorage.getItem(ANONYMOUS_USER_ID_KEY)
        let cohortID = cookies.get(COHORT_ID_KEY)
        if (!anonymousUserID) {
            anonymousUserID = uuid.v4()
            cohortID = getPreviousMonday(new Date())
        }

        // Use cookies instead of localStorage so that the ID can be shared with subdomains (about.sourcegraph.com).
        // Always set to renew expiry and migrate from localStorage
        cookies.set(ANONYMOUS_USER_ID_KEY, anonymousUserID, this.cookieSettings)
        localStorage.removeItem(ANONYMOUS_USER_ID_KEY)
        if (cohortID) {
            cookies.set(COHORT_ID_KEY, cohortID, this.cookieSettings)
        }

        let deviceID = cookies.get(DEVICE_ID_KEY)
        if (!deviceID) {
            // If device ID does not exist, use the anonymous user ID value so these are consolidated.
            deviceID = anonymousUserID
            cookies.set(DEVICE_ID_KEY, deviceID, this.cookieSettings)
        }

        this.anonymousUserID = anonymousUserID
        this.cohortID = cohortID
        this.deviceID = deviceID
    }

    public addEventLogListener(callback: (eventName: string) => void): () => void {
        this.listeners.add(callback)
        return () => this.listeners.delete(callback)
    }
}

export const eventLogger = new EventLogger()

/**
 * Log events associated with URL query string parameters, and remove those parameters as necessary
 * Note that this is a destructive operation (it changes the page URL and replaces browser state) by
 * calling stripURLParameters
 */
function handleQueryEvents(url: string): void {
    const parsedUrl = new URL(url)
    const isBadgeRedirect = !!parsedUrl.searchParams.get('badge')
    if (isBadgeRedirect) {
        eventLogger.log('RepoBadgeRedirected')
    }

    stripURLParameters(url, ['utm_campaign', 'utm_source', 'utm_medium', 'badge'])
}

/**
 * Get pageview-specific event properties from URL query string parameters
 */
function pageViewQueryParameters(url: string): UTMMarker {
    const parsedUrl = new URL(url)

    const utmSource = parsedUrl.searchParams.get('utm_source')
    const utmCampaign = parsedUrl.searchParams.get('utm_campaign')
    const utmMedium = parsedUrl.searchParams.get('utm_medium')

    const utmProps: UTMMarker = {
        utm_campaign: utmCampaign || undefined,
        utm_source: utmSource || undefined,
        utm_medium: utmMedium || undefined,
        utm_term: parsedUrl.searchParams.get('utm_term') || undefined,
        utm_content: parsedUrl.searchParams.get('utm_content') || undefined,
    }

    if (utmSource === 'saved-search-email') {
        eventLogger.log('SavedSearchEmailClicked')
    } else if (utmSource === 'saved-search-slack') {
        eventLogger.log('SavedSearchSlackClicked')
    } else if (utmSource === 'code-monitoring-email') {
        eventLogger.log('CodeMonitorEmailLinkClicked')
    } else if (utmSource === 'hubspot' && utmCampaign?.match(/^cloud-onboarding-email(.*)$/)) {
        eventLogger.log('UTMCampaignLinkClicked', utmProps, utmProps)
    } else if (
        [
            'safari-extension',
            'firefox-extension',
            'chrome-extension',
            'phabricator-integration',
            'bitbucket-integration',
            'gitlab-integration',
        ].includes(utmSource ?? '')
    ) {
        eventLogger.log('UTMCodeHostIntegration', utmProps, utmProps)
    } else if (utmMedium === 'VSCIDE' && utmCampaign === 'vsce-sign-up') {
        eventLogger.log('VSCIDESignUpLinkClicked', utmProps, utmProps)
    }

    return utmProps
}
