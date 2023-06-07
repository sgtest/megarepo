import { Optional } from 'utility-types'

import { BatchChangeState } from '../../graphql-operations'

import { DiffMode } from './diffMode'
import { RecentSearch } from './recentSearches'
import { SectionID, NoResultsSectionID } from './searchSidebar'
import { TourListState } from './tourState'

// Prior to this type we store in settings list of MultiSelectState
// we no longer use MultiSelect UI but for backward compatibility we still
// have to store and parse the old version of batch changes filters
export interface LegacyBatchChangesFilter {
    label: string
    value: BatchChangeState
}

/**
 * Schema for temporary settings.
 */
export interface TemporarySettingsSchema {
    'search.collapsedSidebarSections': { [key in SectionID]?: boolean }
    'search.hiddenNoResultsSections': NoResultsSectionID[]
    'search.sidebar.revisions.tab': number
    'search.sidebar.collapsed': boolean // Used only on non-mobile sizes and when coreWorkflowImprovements.enabled is set
    'search.notepad.enabled': boolean
    'search.notepad.ctaSeen': boolean
    'search.notebooks.gettingStartedTabSeen': boolean
    'insights.freeGaAccepted': boolean
    'insights.freeGaExpiredAccepted': boolean
    'insights.wasMainPageOpen': boolean
    'insights.lastVisitedDashboardId': string | null
    'npsSurvey.hasTemporarilyDismissed': boolean
    'npsSurvey.hasPermanentlyDismissed': boolean
    'user.lastDayActive': string | null
    'user.daysActiveCount': number
    'user.themePreference': string
    'signup.finishedWelcomeFlow': boolean
    'homepage.userInvites.tab': number
    'batches.defaultListFilters': LegacyBatchChangesFilter[]
    'batches.downloadSpecModalDismissed': boolean
    'codeintel.badge.used': boolean
    'codeintel.referencePanel.redesign.ctaDismissed': boolean
    'onboarding.quickStartTour': TourListState
    'characterKeyShortcuts.enabled': boolean
    'search.homepage.queryExamplesContent': {
        lastCachedTimestamp: string
        repositoryName: string
        filePath: string
        author: string
    }
    'search.results.collapseSmartSearch': boolean
    'search.results.collapseUnownedResultsAlert': boolean
    'search.input.recentSearches': RecentSearch[]
    /**
     * Keeps track of which of the query examples shown as suggestions
     * the user has used so that we don't suggest them anymore.
     */
    'search.input.usedExamples': string[]
    'search.input.usedInlineHistory': boolean
    // This is a temporary (no pun intended) setting to allow users to easily
    // switch been the current and the new search input. It's only used when
    // the feature flag `"searchQueryInput": "experimental"` is set.
    'search.input.experimental': boolean
    'batches.minSavedPerChangeset': number
    'search.notebooks.minSavedPerView': number
    'repo.commitPage.diffMode': DiffMode
    'setup.activeStepId': string
    'app-setup.activeStepId': string
    'own.panelExplanationHidden': boolean
    'cody.showSidebar': boolean
    'cody.blobPageCta.dismissed': boolean
    'cody.searchPageCta.dismissed': boolean
    'cody.chatPageCta.dismissed': boolean
    'cody.survey.submitted': boolean
    'app.codyStandalonePage.selectedRepo': string
}

/**
 * All temporary settings are possibly undefined. This is the actual schema that
 * should be used to force the consumer to check for undefined values.
 */
export type TemporarySettings = Optional<TemporarySettingsSchema>
