import { isErrorLike } from '@sourcegraph/common'
import { SettingsExperimentalFeatures } from '@sourcegraph/shared/src/schema/settings.schema'
import { SettingsCascadeOrError } from '@sourcegraph/shared/src/settings/settings'

/**
 * Code insights display location setting to check setting for particular view
 * to show code insights components.
 */
interface CodeInsightsDisplayLocation {
    homepage: boolean
    directory: boolean
}

/**
 * Feature guard for code insights.
 *
 * @param settingsCascade - settings cascade object
 * @param views - Map with display location of insights {@link CodeInsightsDisplayLocation}
 */
export function isCodeInsightsEnabled(
    settingsCascade: SettingsCascadeOrError,
    views: Partial<CodeInsightsDisplayLocation> = {}
): boolean {
    if (isErrorLike(settingsCascade.final)) {
        return false
    }

    const final = settingsCascade.final
    const viewsKeys = Object.keys(views) as (keyof CodeInsightsDisplayLocation)[]
    const experimentalFeatures: SettingsExperimentalFeatures = final?.experimentalFeatures ?? {}

    if (experimentalFeatures.codeInsights === false) {
        return false
    }

    return viewsKeys.every(viewKey => {
        if (views[viewKey]) {
            return !!final?.[`insights.displayLocation.${viewKey}`]
        }

        return true
    })
}
