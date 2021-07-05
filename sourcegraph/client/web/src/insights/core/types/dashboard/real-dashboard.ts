import { ExtendedInsightDashboard } from './core'

/**
 * Derived dashboard from the setting cascade subject.
 */
interface BuiltInInsightDashboard extends ExtendedInsightDashboard {
    /**
     * Property to distinguish between real user-created dashboard and virtual
     * built-in dashboard. Currently we support 2 types of user built-in dashboard.
     *
     * "Personal" - all personal insights from personal settings (all users
     * have it by default)
     *
     * "Organizations level" - all organizations act as an insights dashboard.
     */
    builtIn: true
}

/**
 * Explicitly created in the settings cascade insights dashboard.
 */
interface SettingsBasedInsightDashboard extends ExtendedInsightDashboard {
    /**
     * Value of dashboard key in the settings for which the dashboard data is available.
     * Dashboard already has an id property but this id is UUID and will be used for further
     * BE migration.
     */
    settingsKey: string
}

/**
 * Insights dashboards that were created in a user/org settings.
 */
export type RealInsightDashboard = SettingsBasedInsightDashboard | BuiltInInsightDashboard
