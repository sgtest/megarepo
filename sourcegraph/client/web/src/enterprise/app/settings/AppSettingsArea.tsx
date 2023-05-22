import { FC } from 'react'

import { Routes, Route, Outlet, Navigate, useLocation } from 'react-router-dom'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Button, Link, PageHeader } from '@sourcegraph/wildcard'

import { RemoteRepositoriesStep } from '../../../setup-wizard/components'

import { LocalRepositoriesTab } from './local-repositories/LocalRepositoriesTab'

import styles from './AppSettingsArea.module.scss'

enum AppSettingURL {
    LocalRepositories = 'local-repositories',
    RemoteRepositories = 'remote-repositories',
}

export const AppSettingsArea: FC<TelemetryProps> = ({ telemetryService }) => (
    <Routes>
        <Route path="*" element={<AppSettingsLayout />}>
            <Route path={AppSettingURL.LocalRepositories} element={<LocalRepositoriesTab />} />
            <Route
                path={`${AppSettingURL.RemoteRepositories}/*`}
                element={<RemoteRepositoriesTab telemetryService={telemetryService} />}
            />
            <Route path="*" element={<Navigate to={AppSettingURL.LocalRepositories} replace={true} />} />
        </Route>
    </Routes>
)

interface AppSetting {
    url: AppSettingURL
    name: string
}

const APP_SETTINGS: AppSetting[] = [
    { url: AppSettingURL.LocalRepositories, name: 'Local repositories' },
    { url: AppSettingURL.RemoteRepositories, name: 'Remote repositories' },
]

const AppSettingsLayout: FC = () => {
    const location = useLocation()

    return (
        <div className={styles.root}>
            <ul className={styles.navigation}>
                {APP_SETTINGS.map(setting => (
                    <li key={setting.url}>
                        <Button
                            as={Link}
                            to={`../${setting.url}`}
                            variant={location.pathname.includes(`/${setting.url}`) ? 'primary' : undefined}
                            className={styles.navigationItemLink}
                        >
                            {setting.name}
                        </Button>
                    </li>
                ))}
            </ul>

            <Outlet />
        </div>
    )
}

const RemoteRepositoriesTab: FC<TelemetryProps> = ({ telemetryService }) => (
    <div className={styles.content}>
        <PageHeader
            headingElement="h2"
            path={[{ text: 'Remote repositories' }]}
            description="Add your remote repositories from GitHub, GitLab or Bitbucket"
            className="mb-3"
        />

        <RemoteRepositoriesStep
            baseURL={`app-settings/${AppSettingURL.RemoteRepositories}`}
            description={false}
            progressBar={false}
            telemetryService={telemetryService}
        />
    </div>
)
