import MapSearchIcon from 'mdi-react/MapSearchIcon'
import * as React from 'react'
import { Route, RouteComponentProps, Switch } from 'react-router'
import { ExtensionsControllerProps } from '../../../shared/src/extensions/controller'
import * as GQL from '../../../shared/src/graphql/schema'
import { PlatformContextProps } from '../../../shared/src/platform/context'
import { SettingsCascadeProps } from '../../../shared/src/settings/settings'
import { HeroPage } from '../components/HeroPage'
import { ThemeProps } from '../theme'
import { OrgArea, OrgAreaRoute } from './area/OrgArea'
import { OrgAreaHeaderNavItem } from './area/OrgHeader'
import { NewOrganizationPage } from './new/NewOrganizationPage'

const NotFoundPage = () => (
    <HeroPage
        icon={MapSearchIcon}
        title="404: Not Found"
        subtitle="Sorry, the requested organization page was not found."
    />
)

interface Props
    extends RouteComponentProps<any>,
        ExtensionsControllerProps,
        PlatformContextProps,
        SettingsCascadeProps,
        ThemeProps {
    orgAreaRoutes: ReadonlyArray<OrgAreaRoute>
    orgAreaHeaderNavItems: ReadonlyArray<OrgAreaHeaderNavItem>

    authenticatedUser: GQL.IUser | null
}

/**
 * Renders a layout of a sidebar and a content area to display organization-related pages.
 */
export const OrgsArea: React.FunctionComponent<Props> = props => (
    <Switch>
        <Route path={`${props.match.url}/new`} component={NewOrganizationPage} exact={true} />
        <Route
            path={`${props.match.url}/:name`}
            // tslint:disable-next-line: jsx-no-lambda
            render={routeComponentProps => <OrgArea {...props} {...routeComponentProps} />}
        />
        <Route component={NotFoundPage} />
    </Switch>
)
