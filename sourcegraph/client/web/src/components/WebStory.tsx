import React, { useMemo } from 'react'
import { MemoryRouter, MemoryRouterProps, RouteComponentProps, withRouter } from 'react-router'

import { Tooltip } from '@sourcegraph/branded/src/components/tooltip/Tooltip'
import { NOOP_TELEMETRY_SERVICE, TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { usePrependStyles } from '@sourcegraph/storybook/src/hooks/usePrependStyles'
import { useTheme } from '@sourcegraph/storybook/src/hooks/useTheme'

import webStyles from '../SourcegraphWebApp.scss'

import { BreadcrumbSetters, BreadcrumbsProps, useBreadcrumbs } from './Breadcrumbs'

export interface WebStoryProps extends MemoryRouterProps {
    children: React.FunctionComponent<
        ThemeProps & BreadcrumbSetters & BreadcrumbsProps & TelemetryProps & RouteComponentProps<any>
    >
    additionalWebStyles?: string
}

/**
 * Wrapper component for webapp Storybook stories that provides light theme and react-router props.
 * Takes a render function as children that gets called with the props.
 */
export const WebStory: React.FunctionComponent<WebStoryProps> = ({
    children,
    additionalWebStyles,
    ...memoryRouterProps
}) => {
    const isLightTheme = useTheme()
    const breadcrumbSetters = useBreadcrumbs()
    const Children = useMemo(() => withRouter(children), [children])

    usePrependStyles('additional-web-styles', additionalWebStyles)
    usePrependStyles('web-styles', webStyles)

    return (
        <MemoryRouter {...memoryRouterProps}>
            <Tooltip />
            <Children {...breadcrumbSetters} isLightTheme={isLightTheme} telemetryService={NOOP_TELEMETRY_SERVICE} />
        </MemoryRouter>
    )
}
