import MapSearchIcon from 'mdi-react/MapSearchIcon'
import React from 'react'
import { RouteComponentProps, Switch, Route } from 'react-router'

import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'

import { AuthenticatedUser } from '../../../auth'
import { withAuthenticatedUser } from '../../../auth/withAuthenticatedUser'
import { HeroPage } from '../../../components/HeroPage'
import { Page } from '../../../components/Page'
import { lazyComponent } from '../../../util/lazyComponent'
import { BatchChangeClosePageProps } from '../close/BatchChangeClosePage'
import { CreateBatchChangePageProps } from '../create/CreateBatchChangePage'
import { BatchChangeDetailsPageProps } from '../detail/BatchChangeDetailsPage'
import { BatchChangeListPageProps, NamespaceBatchChangeListPageProps } from '../list/BatchChangeListPage'
import { BatchChangePreviewPageProps } from '../preview/BatchChangePreviewPage'

const BatchChangeListPage = lazyComponent<BatchChangeListPageProps, 'BatchChangeListPage'>(
    () => import('../list/BatchChangeListPage'),
    'BatchChangeListPage'
)
const NamespaceBatchChangeListPage = lazyComponent<NamespaceBatchChangeListPageProps, 'NamespaceBatchChangeListPage'>(
    () => import('../list/BatchChangeListPage'),
    'NamespaceBatchChangeListPage'
)
const BatchChangePreviewPage = lazyComponent<BatchChangePreviewPageProps, 'BatchChangePreviewPage'>(
    () => import('../preview/BatchChangePreviewPage'),
    'BatchChangePreviewPage'
)
const CreateBatchChangePage = lazyComponent<CreateBatchChangePageProps, 'CreateBatchChangePage'>(
    () => import('../create/CreateBatchChangePage'),
    'CreateBatchChangePage'
)
const BatchChangeDetailsPage = lazyComponent<BatchChangeDetailsPageProps, 'BatchChangeDetailsPage'>(
    () => import('../detail/BatchChangeDetailsPage'),
    'BatchChangeDetailsPage'
)
const BatchChangeClosePage = lazyComponent<BatchChangeClosePageProps, 'BatchChangeClosePage'>(
    () => import('../close/BatchChangeClosePage'),
    'BatchChangeClosePage'
)

const RedirectToMarketing: React.FunctionComponent<{}> = () => {
    window.location.href = 'https://about.sourcegraph.com/batch-changes'
    return null
}

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
 * The global batch changes area.
 */
export const GlobalBatchChangesArea: React.FunctionComponent<Props> = props => {
    if (props.isSourcegraphDotCom) {
        return <RedirectToMarketing />
    }
    return <AuthenticatedBatchChangesArea {...props} />
}

const NotFoundPage: React.FunctionComponent = () => <HeroPage icon={MapSearchIcon} title="404: Not Found" />

interface AuthenticatedProps extends Props {
    authenticatedUser: AuthenticatedUser
}

export const AuthenticatedBatchChangesArea = withAuthenticatedUser<AuthenticatedProps>(({ match, ...outerProps }) => (
    <Page>
        {/* eslint-disable react/jsx-no-bind */}
        <Switch>
            <Route render={props => <BatchChangeListPage {...outerProps} {...props} />} path={match.url} exact={true} />
            <Route
                path={`${match.url}/create`}
                render={props => <CreateBatchChangePage {...outerProps} {...props} />}
                exact={true}
            />
            <Route component={NotFoundPage} key="hardcoded-key" />
        </Switch>
        {/* eslint-enable react/jsx-no-bind */}
    </Page>
))

export interface NamespaceBatchChangesAreaProps extends Props {
    namespaceID: Scalars['ID']
}

export const NamespaceBatchChangesArea = withAuthenticatedUser<
    NamespaceBatchChangesAreaProps & { authenticatedUser: AuthenticatedUser }
>(({ match, namespaceID, ...outerProps }) => (
    <Page>
        {/* eslint-disable react/jsx-no-bind */}
        <Switch>
            <Route
                path={`${match.url}/apply/:specID`}
                render={({ match, ...props }: RouteComponentProps<{ specID: string }>) => (
                    <BatchChangePreviewPage {...outerProps} {...props} batchSpecID={match.params.specID} />
                )}
            />
            <Route
                path={`${match.url}/create`}
                render={props => <CreateBatchChangePage {...outerProps} {...props} />}
            />
            <Route
                path={`${match.url}/:batchChangeName/close`}
                render={({ match, ...props }: RouteComponentProps<{ batchChangeName: string }>) => (
                    <BatchChangeClosePage
                        {...outerProps}
                        {...props}
                        namespaceID={namespaceID}
                        batchChangeName={match.params.batchChangeName}
                    />
                )}
            />
            <Route
                path={`${match.url}/:batchChangeName`}
                render={({ match, ...props }: RouteComponentProps<{ batchChangeName: string }>) => (
                    <BatchChangeDetailsPage
                        {...outerProps}
                        {...props}
                        namespaceID={namespaceID}
                        batchChangeName={match.params.batchChangeName}
                    />
                )}
            />
            <Route
                path={match.url}
                render={props => <NamespaceBatchChangeListPage {...props} {...outerProps} namespaceID={namespaceID} />}
                exact={true}
            />
        </Switch>
        {/* eslint-enable react/jsx-no-bind */}
    </Page>
))
