import { take } from 'rxjs/operators'

import { TemporarySettings, TemporarySettingsSchema } from './TemporarySettings'
import { TemporarySettingsStorage } from './TemporarySettingsStorage'

interface Migration {
    localStorageKey: string
    temporarySettingsKey: keyof TemporarySettings
    type: 'boolean' | 'number' | 'json'
    transform?: (value: any) => any
    preserve?: boolean
}

const migrations: Migration[] = [
    {
        localStorageKey: 'days-active-count',
        temporarySettingsKey: 'user.daysActiveCount',
        type: 'number',
    },
    {
        localStorageKey: 'has-dismissed-survey-toast',
        temporarySettingsKey: 'npsSurvey.hasTemporarilyDismissed',
        type: 'boolean',
    },
    {
        localStorageKey: 'has-permanently-dismissed-survey-toast',
        temporarySettingsKey: 'npsSurvey.hasPermanentlyDismissed',
        type: 'boolean',
    },
    {
        localStorageKey: 'finished-welcome-flow',
        temporarySettingsKey: 'signup.finishedWelcomeFlow',
        type: 'boolean',
    },
    {
        localStorageKey: 'hasDismissedBrowserExtensionAlert',
        temporarySettingsKey: 'cta.browserExtensionAlertDismissed',
        type: 'boolean',
    },
    {
        localStorageKey: 'hasDismissedIdeExtensionAlert',
        temporarySettingsKey: 'cta.ideExtensionAlertDismissed',
        type: 'boolean',
    },
    {
        localStorageKey: 'quick-start-tour',
        temporarySettingsKey: 'onboarding.quickStartTour',
        type: 'json',
        transform: (value: { state: { tours: TemporarySettingsSchema['onboarding.quickStartTour'] } }) =>
            value.state.tours,
        preserve: true,
    },
]

const parse = (type: Migration['type'], localStorageValue: string | null): boolean | number | any => {
    if (localStorageValue === null) {
        return
    }
    if (type === 'boolean') {
        return localStorageValue === 'true'
    }

    if (type === 'number') {
        return parseInt(localStorageValue, 10)
    }

    if (type === 'json') {
        return JSON.parse(localStorageValue)
    }
    return
}

export async function migrateLocalStorageToTemporarySettings(storage: TemporarySettingsStorage): Promise<void> {
    for (const migration of migrations) {
        // Use the first value of the setting to check if it exists.
        // Only migrate if the setting is not already set.
        const temporarySetting = await storage.get(migration.temporarySettingsKey).pipe(take(1)).toPromise()
        if (typeof temporarySetting === 'undefined') {
            try {
                // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
                const value = parse(migration.type, localStorage.getItem(migration.localStorageKey))
                if (!value) {
                    continue
                }

                storage.set(migration.temporarySettingsKey, migration.transform?.(value) ?? value)
                if (!migration.preserve) {
                    localStorage.removeItem(migration.localStorageKey)
                }
            } catch (error) {
                console.error(
                    `Failed to migrate temporary settings "${migration.temporarySettingsKey}" from localStorage using key "${migration.localStorageKey}"`,
                    error
                )
            }
        }
    }
}
