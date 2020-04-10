// This is the entry point for the web app

// Order is important here
// Don't remove the empty lines between these imports

import '../../shared/src/polyfills'

import './sentry'

import React from 'react'
import { render } from 'react-dom'
import { exploreSections } from './explore/exploreSections'
import { extensionAreaHeaderNavItems } from './extensions/extension/extensionAreaHeaderNavItems'
import { extensionAreaRoutes } from './extensions/extension/routes'
import { extensionsAreaHeaderActionButtons } from './extensions/extensionsAreaHeaderActionButtons'
import { extensionsAreaRoutes } from './extensions/routes'
import './main.scss'
import { orgAreaHeaderNavItems } from './org/area/navitems'
import { orgAreaRoutes } from './org/area/routes'
import { repoHeaderActionButtons } from './repo/repoHeaderActionButtons'
import { repoContainerRoutes, repoRevContainerRoutes } from './repo/routes'
import { routes } from './routes'
import { siteAdminOverviewComponents } from './site-admin/overview/overviewComponents'
import { siteAdminAreaRoutes } from './site-admin/routes'
import { siteAdminSidebarGroups } from './site-admin/sidebaritems'
import { SourcegraphWebApp } from './SourcegraphWebApp'
import { userAreaHeaderNavItems } from './user/area/navitems'
import { userAreaRoutes } from './user/area/routes'
import { userSettingsAreaRoutes } from './user/settings/routes'
import { userSettingsSideBarItems } from './user/settings/sidebaritems'
import { KEYBOARD_SHORTCUTS } from './keyboardShortcuts/keyboardShortcuts'
import { repoSettingsAreaRoutes } from './repo/settings/routes'
import { repoSettingsSidebarItems } from './repo/settings/sidebaritems'

window.addEventListener('DOMContentLoaded', () => {
    render(
        <SourcegraphWebApp
            exploreSections={exploreSections}
            extensionAreaRoutes={extensionAreaRoutes}
            extensionAreaHeaderNavItems={extensionAreaHeaderNavItems}
            extensionsAreaRoutes={extensionsAreaRoutes}
            extensionsAreaHeaderActionButtons={extensionsAreaHeaderActionButtons}
            siteAdminAreaRoutes={siteAdminAreaRoutes}
            siteAdminSideBarGroups={siteAdminSidebarGroups}
            siteAdminOverviewComponents={siteAdminOverviewComponents}
            userAreaRoutes={userAreaRoutes}
            userAreaHeaderNavItems={userAreaHeaderNavItems}
            userSettingsSideBarItems={userSettingsSideBarItems}
            userSettingsAreaRoutes={userSettingsAreaRoutes}
            orgAreaRoutes={orgAreaRoutes}
            orgAreaHeaderNavItems={orgAreaHeaderNavItems}
            repoContainerRoutes={repoContainerRoutes}
            repoRevContainerRoutes={repoRevContainerRoutes}
            repoHeaderActionButtons={repoHeaderActionButtons}
            repoSettingsAreaRoutes={repoSettingsAreaRoutes}
            repoSettingsSidebarItems={repoSettingsSidebarItems}
            routes={routes}
            keyboardShortcuts={KEYBOARD_SHORTCUTS}
            showCampaigns={false}
        />,
        document.querySelector('#root')
    )
})
