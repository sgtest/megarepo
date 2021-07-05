import * as jsonc from '@sqs/jsonc-parser'

import { Insight, InsightTypePrefix, isLangStatsInsight, isSearchBasedInsight } from '../types'

const defaultFormattingOptions: jsonc.FormattingOptions = {
    eol: '\n',
    insertSpaces: true,
    tabSize: 2,
}

/**
 * Returns insights extension name based on insight id.
 */
const getExtensionNameByInsight = (insight: Insight): string | undefined => {
    if (isSearchBasedInsight(insight)) {
        return 'sourcegraph/search-insights'
    }

    if (isLangStatsInsight(insight)) {
        return 'sourcegraph/code-stats-insights'
    }

    return undefined
}

/**
 * Simplified jsonc API method to modify jsonc object.
 *
 * @param originalContent Original content (settings)
 * @param path - path to the field which will be modified
 * @param value - new value for modify field
 */
const modify = (originalContent: string, path: jsonc.JSONPath, value: unknown): string => {
    const addingExtensionKeyEdits = jsonc.modify(originalContent, path, value, {
        formattingOptions: defaultFormattingOptions,
    })

    return jsonc.applyEdits(originalContent, addingExtensionKeyEdits)
}

/**
 * Serializes and adds insight configurations to the settings content string (jsonc).
 * Returns settings content string with serialized insight inside.
 *
 * @param settings - original settings content string
 * @param insight - insight configuration to add in settings file
 */
export const addInsightToSettings = (settings: string, insight: Insight): string => {
    const { id, visibility, ...originalInsight } = insight

    const extensionName = getExtensionNameByInsight(insight)

    if (!extensionName) {
        return settings
    }

    // Turn on extension if user in creation code insight.
    const settingsWithExtension = modify(settings, ['extensions', extensionName], true)

    // Add insight to the user settings
    return modify(settingsWithExtension, [id], originalInsight)
}

interface RemoveInsightFromSettingsInputs {
    originalSettings: string
    insightID: string
    isOldCodeStatsInsight?: boolean
}

/**
 * Return edited settings without deleted insight.
 */
export const removeInsightFromSettings = (props: RemoveInsightFromSettingsInputs): string => {
    const {
        originalSettings,
        insightID,
        // For backward compatibility with old code stats insight api we have to delete
        // this insight in a special way. See link below for more information.
        // https://github.com/sourcegraph/sourcegraph-code-stats-insights/blob/master/src/code-stats-insights.ts#L33
        isOldCodeStatsInsight = insightID === `${InsightTypePrefix.langStats}.language`,
    } = props

    if (isOldCodeStatsInsight) {
        const editedSettings = modify(
            originalSettings,
            // According to our naming convention <type>.insight.<name>
            ['codeStatsInsights.query'],
            undefined
        )

        return modify(
            editedSettings,
            // According to our naming convention <type>.insight.<name>
            ['codeStatsInsights.otherThreshold'],
            undefined
        )
    }

    // Remove insight settings from subject (user/org settings)
    return modify(
        originalSettings,
        // According to our naming convention <type>.insight.<name>
        [insightID],
        undefined
    )
}
