import { Optional } from 'utility-types'

// eslint-disable-next-line no-restricted-imports
import { TourListState } from '@sourcegraph/web/src/tour/components/Tour/useTour'
import { MultiSelectState } from '@sourcegraph/wildcard'

import { BatchChangeState } from '../../graphql-operations'

import { SectionID, NoResultsSectionID } from './searchSidebar'

/**
 * Schema for temporary settings.
 */
export interface TemporarySettingsSchema {
    'search.collapsedSidebarSections': { [key in SectionID]?: boolean }
    'search.hiddenNoResultsSections': NoResultsSectionID[]
    'search.sidebar.revisions.tab': number
    'search.notepad.enabled': boolean
    'search.notepad.ctaSeen': boolean
    'search.notebooks.gettingStartedTabSeen': boolean
    'insights.freeGaAccepted': boolean
    'insights.freeGaExpiredAccepted': boolean
    'insights.wasMainPageOpen': boolean
    'npsSurvey.hasTemporarilyDismissed': boolean
    'npsSurvey.hasPermanentlyDismissed': boolean
    'user.lastDayActive': string | null
    'user.daysActiveCount': number
    'signup.finishedWelcomeFlow': boolean
    'homepage.userInvites.tab': number
    'integrations.vscode.lastDetectionTimestamp': number
    'integrations.jetbrains.lastDetectionTimestamp': number
    'cta.browserExtensionAlertDismissed': boolean
    'cta.ideExtensionAlertDismissed': boolean
    'batches.defaultListFilters': MultiSelectState<BatchChangeState>
    'batches.downloadSpecModalDismissed': boolean
    'codeintel.badge.used': boolean
    'codeintel.referencePanel.redesign.ctaDismissed': boolean
    'codeintel.referencePanel.redesign.enabled': boolean
    'onboarding.quickStartTour': TourListState
    'coreWorkflowImprovements.enabled': boolean
}

/**
 * All temporary settings are possibly undefined. This is the actual schema that
 * should be used to force the consumer to check for undefined values.
 */
export type TemporarySettings = Optional<TemporarySettingsSchema>
