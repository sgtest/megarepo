import type { AuthProvider } from '../jscontext'

// NOTE(naman): Remember to add events to allow list: https://docs.sourcegraph.com/dev/background-information/data-usage-pipeline#allow-list
export const enum EventName {
    CODY_CHAT_PAGE_VIEWED = 'web:codyChat:pageViewed',
    CODY_CHAT_SUBMIT = 'web:codyChat:submit',
    CODY_CHAT_EDIT = 'web:codyChat:edit',
    CODY_CHAT_INITIALIZED = 'web:codyChat:initialized',
    CODY_CHAT_EDITOR_WIDGET_VIEWED = 'web:codyChat:editorWidgetViewed',
    CODY_CHAT_HISTORY_CLEARED = 'web:codyChat:historyCleared',
    CODY_CHAT_HISTORY_ITEM_DELETED = 'web:codyChat:historyItemDeleted',

    CODY_CHAT_SCOPE_REPO_ADDED = 'web:codyChat:scopeRepoAdded',
    CODY_CHAT_SCOPE_REPO_REMOVED = 'web:codyChat:scopeRepoRemoved',
    CODY_CHAT_SCOPE_RESET = 'web:codyChat:scopeReset',
    CODY_CHAT_SCOPE_INFERRED_REPO_ENABLED = 'web:codyChat:inferredRepoEnabled',
    CODY_CHAT_SCOPE_INFERRED_REPO_DISABLED = 'web:codyChat:inferredRepoDisabled',
    CODY_CHAT_SCOPE_INFERRED_FILE_ENABLED = 'web:codyChat:inferredFileEnabled',
    CODY_CHAT_SCOPE_INFERRED_FILE_DISABLED = 'web:codyChat:inferredFileDisabled',
    VIEW_GET_CODY = 'GetCody',

    CODY_EDITOR_WIDGET_VIEWED = 'web:codyEditorWidget:viewed',
    CODY_SIDEBAR_CHAT_OPENED = 'web:codySidebar:chatOpened',
    CODY_SIGNUP_CTA_CLICK = 'CodySignUpCTAClick',
    CODY_CHAT_DOWNLOAD_VSCODE = 'web:codyChat:downloadVSCodeCTA',
    CODY_CHAT_GET_EDITOR_EXTENSION = 'web:codyChat:getEditorExtensionCTA',
    CODY_CHAT_TRY_ON_PUBLIC_CODE = 'web:codyChat:tryOnPublicCodeCTA',
    CODY_CTA = 'ClickedOnCodyCTA',
    VIEW_EDITOR_EXTENSIONS = 'CodyClickViewEditorExtensions',
    TRY_CODY_VSCODE = 'VSCodeInstall',
    TRY_CODY_MARKETPLACE = 'VSCodeMarketplace',
    TRY_CODY_WEB = 'TryCodyWeb',
    TRY_CODY_WEB_ONBOARDING_DISPLAYED = 'TryCodyWebOnboardingDisplayed',
    TRY_CODY_SIGNUP_INITIATED = 'CodySignUpInitiated',
    SPEAK_TO_AN_ENGINEER_CTA = 'SpeakToACodyEngineerCTA',
    AUTH_INITIATED = 'AuthInitiated',
    SIGNUP_COMPLETED = 'web:auth:signUpCompleted',
    SINGIN_COMPLETED = 'web:auth:signInCompleted',

    JOIN_IDE_WAITLIST = 'JoinIDEWaitlist',
    DOWNLOAD_IDE = 'DownloadIDE',
    DOWNLOAD_APP = 'DownloadApp',

    CODY_MANAGEMENT_PAGE_VIEWED = 'CodyManageViewed',
    CODY_SUBSCRIPTION_PAGE_VIEWED = 'CodyPlanSelectionViewed',
    CODY_SUBSCRIPTION_PLAN_CLICKED = 'CodyPlanSelectionClicked',
    CODY_SUBSCRIPTION_PLAN_CONFIRMED = 'CodyPlanSelectionConfirmed',
    CODY_SUBSCRIPTION_ADD_CREDIT_CARD_CLICKED = 'CodyAddCreditCard',
    CODY_MANAGE_SUBSCRIPTION_CLICKED = 'CodyManageSubscriptionClicked',
    CODY_ONBOARDING_WELCOME_VIEWED = 'CodyWelcomeViewed',
    CODY_ONBOARDING_PURPOSE_VIEWED = 'CodyUseCaseViewed',
    CODY_ONBOARDING_PURPOSE_SELECTED = 'CodyUseCaseSelected',
    CODY_ONBOARDING_CHOOSE_EDITOR_VIEWED = 'CodyEditorViewed',
    CODY_ONBOARDING_CHOOSE_EDITOR_SKIPPED = 'CodyEditorSkipped',
    CODY_ONBOARDING_CHOOSE_EDITOR_SELECTED = 'CodyEditorSelected',
}

export const enum EventLocation {
    NAV_BAR = 'NavBar',
    CHAT_RESPONSE = 'ChatResponse',
}

export const V2AuthProviderTypes: { [k in AuthProvider['serviceType']]: number } = {
    github: 0,
    gitlab: 1,
    bitbucketCloud: 2,
    'http-header': 3,
    openidconnect: 4,
    'sourcegraph-operator': 5,
    saml: 6,
    builtin: 7,
    gerrit: 8,
    azuredevops: 9,
}
