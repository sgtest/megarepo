import React, { useMemo } from 'react'
import { useDarkMode } from 'storybook-dark-mode'
import { MemoryRouter, MemoryRouterProps, RouteComponentProps, withRouter } from 'react-router'
import { NOOP_TELEMETRY_SERVICE, TelemetryProps } from '../../../shared/src/telemetry/telemetryService'
import { ThemeProps } from '../../../shared/src/theme'
import _webStyles from '../SourcegraphWebApp.scss'
import { BreadcrumbSetters, BreadcrumbsProps, useBreadcrumbs } from './Breadcrumbs'
import { Tooltip } from '../../../branded/src/components/tooltip/Tooltip'

export interface WebStoryProps extends MemoryRouterProps {
    children: React.FunctionComponent<
        ThemeProps & BreadcrumbSetters & BreadcrumbsProps & TelemetryProps & RouteComponentProps<any>
    >
}

/**
 * Wrapper component for webapp Storybook stories that provides light theme and react-router props.
 * Takes a render function as children that gets called with the props.
 */
export const WebStory: React.FunctionComponent<
    WebStoryProps & {
        webStyles?: string
    }
> = ({ children, webStyles = _webStyles, ...memoryRouterProps }) => {
    const isLightTheme = !useDarkMode()
    const breadcrumbSetters = useBreadcrumbs()
    const Children = useMemo(() => withRouter(children), [children])
    return (
        <MemoryRouter {...memoryRouterProps}>
            <Tooltip />
            <Children {...breadcrumbSetters} isLightTheme={isLightTheme} telemetryService={NOOP_TELEMETRY_SERVICE} />
            <style title="Webapp CSS">{webStyles}</style>
        </MemoryRouter>
    )
}
