import { Optional } from 'utility-types'

import { SectionID } from '../../search/results/sidebar/SearchSidebar'

/**
 * Schema for temporary settings.
 */
export interface TemporarySettingsSchema {
    'search.collapsedSidebarSections': { [key in SectionID]?: boolean }
    'search.sidebar.revisions.tab': number
    'insights.freeBetaAccepted': boolean
}

/**
 * All temporary setttings are possibly undefined. This is the actual schema that
 * should be used to force the consumer to check for undefined values.
 */
export type TemporarySettings = Optional<TemporarySettingsSchema>
