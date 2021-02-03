import React from 'react'
import { RouteComponentProps, Switch, Route } from 'react-router'
import { withAuthenticatedUser } from '../../../auth/withAuthenticatedUser'
import { ThemeProps } from '../../../../../shared/src/theme'
import { ExtensionsControllerProps } from '../../../../../shared/src/extensions/controller'
import { PlatformContextProps } from '../../../../../shared/src/platform/context'
import { TelemetryProps } from '../../../../../shared/src/telemetry/telemetryService'
import { AuthenticatedUser } from '../../../auth'
import { Scalars } from '../../../../../shared/src/graphql-operations'
import { lazyComponent } from '../../../util/lazyComponent'
import { CampaignListPageProps, NamespaceCampaignListPageProps } from '../list/CampaignListPage'
import { CampaignPreviewPageProps } from '../preview/CampaignPreviewPage'
import { CreateCampaignPageProps } from '../create/CreateCampaignPage'
import { CampaignDetailsPageProps } from '../detail/CampaignDetailsPage'
import { CampaignClosePageProps } from '../close/CampaignClosePage'
import { CampaignsDotComPageProps } from './marketing/CampaignsDotComPage'
import { Page } from '../../../components/Page'

const CampaignListPage = lazyComponent<CampaignListPageProps, 'CampaignListPage'>(
    () => import('../list/CampaignListPage'),
    'CampaignListPage'
)
const NamespaceCampaignListPage = lazyComponent<NamespaceCampaignListPageProps, 'NamespaceCampaignListPage'>(
    () => import('../list/CampaignListPage'),
    'NamespaceCampaignListPage'
)
const CampaignApplyPage = lazyComponent<CampaignPreviewPageProps, 'CampaignPreviewPage'>(
    () => import('../preview/CampaignPreviewPage'),
    'CampaignPreviewPage'
)
const CreateCampaignPage = lazyComponent<CreateCampaignPageProps, 'CreateCampaignPage'>(
    () => import('../create/CreateCampaignPage'),
    'CreateCampaignPage'
)
const CampaignDetailsPage = lazyComponent<CampaignDetailsPageProps, 'CampaignDetailsPage'>(
    () => import('../detail/CampaignDetailsPage'),
    'CampaignDetailsPage'
)
const CampaignClosePage = lazyComponent<CampaignClosePageProps, 'CampaignClosePage'>(
    () => import('../close/CampaignClosePage'),
    'CampaignClosePage'
)
const CampaignsDotComPage = lazyComponent<CampaignsDotComPageProps, 'CampaignsDotComPage'>(
    () => import('./marketing/CampaignsDotComPage'),
    'CampaignsDotComPage'
)

interface Props
    extends RouteComponentProps<{}>,
        ThemeProps,
        ExtensionsControllerProps,
        TelemetryProps,
        PlatformContextProps {
    authenticatedUser: AuthenticatedUser | null
    isSourcegraphDotCom: boolean
}

/**
 * The global campaigns area.
 */
export const GlobalCampaignsArea: React.FunctionComponent<Props> = props => {
    if (props.isSourcegraphDotCom) {
        return (
            <Page>
                <CampaignsDotComPage />
            </Page>
        )
    }
    return <AuthenticatedCampaignsArea {...props} />
}

interface AuthenticatedProps extends Props {
    authenticatedUser: AuthenticatedUser
}

export const AuthenticatedCampaignsArea = withAuthenticatedUser<AuthenticatedProps>(({ match, ...outerProps }) => (
    <Page>
        {/* eslint-disable react/jsx-no-bind */}
        <Switch>
            <Route render={props => <CampaignListPage {...outerProps} {...props} />} path={match.url} exact={true} />
            <Route
                path={`${match.url}/create`}
                render={props => <CreateCampaignPage {...outerProps} {...props} />}
                exact={true}
            />
        </Switch>
        {/* eslint-enable react/jsx-no-bind */}
    </Page>
))

export interface NamespaceCampaignsAreaProps extends Props {
    namespaceID: Scalars['ID']
}

export const NamespaceCampaignsArea = withAuthenticatedUser<
    NamespaceCampaignsAreaProps & { authenticatedUser: AuthenticatedUser }
>(({ match, namespaceID, ...outerProps }) => (
    <Page>
        {/* eslint-disable react/jsx-no-bind */}
        <Switch>
            <Route
                path={`${match.url}/apply/:specID`}
                render={({ match, ...props }: RouteComponentProps<{ specID: string }>) => (
                    <CampaignApplyPage {...outerProps} {...props} campaignSpecID={match.params.specID} />
                )}
            />
            <Route path={`${match.url}/create`} render={props => <CreateCampaignPage {...outerProps} {...props} />} />
            <Route
                path={`${match.url}/:campaignName/close`}
                render={({ match, ...props }: RouteComponentProps<{ campaignName: string }>) => (
                    <CampaignClosePage
                        {...outerProps}
                        {...props}
                        namespaceID={namespaceID}
                        campaignName={match.params.campaignName}
                    />
                )}
            />
            <Route
                path={`${match.url}/:campaignName`}
                render={({ match, ...props }: RouteComponentProps<{ campaignName: string }>) => (
                    <CampaignDetailsPage
                        {...outerProps}
                        {...props}
                        namespaceID={namespaceID}
                        campaignName={match.params.campaignName}
                    />
                )}
            />
            <Route
                path={match.url}
                render={props => <NamespaceCampaignListPage {...props} {...outerProps} namespaceID={namespaceID} />}
                exact={true}
            />
        </Switch>
        {/* eslint-enable react/jsx-no-bind */}
    </Page>
))
