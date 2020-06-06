import { RepoSettingsSideBarItems } from '../../../repo/settings/RepoSettingsSidebar'
import { repoSettingsSidebarItems } from '../../../repo/settings/sidebaritems'

export const enterpriseRepoSettingsSidebarItems: RepoSettingsSideBarItems = [
    ...repoSettingsSidebarItems,
    {
        to: '/code-intelligence',
        label: 'Code intelligence',
    },
    {
        to: '/permissions',
        exact: true,
        label: 'Permissions',
        condition: () => !!window.context.site['permissions.backgroundSync']?.enabled,
    },
]
