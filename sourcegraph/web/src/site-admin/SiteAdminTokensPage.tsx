import AddIcon from 'mdi-react/AddIcon'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Link } from 'react-router-dom'
import { Observable, Subject } from 'rxjs'
import { map } from 'rxjs/operators'
import { gql } from '../../../shared/src/graphql/graphql'
import * as GQL from '../../../shared/src/graphql/schema'
import { createAggregateError } from '../../../shared/src/util/errors'
import { queryGraphQL } from '../backend/graphql'
import { PageTitle } from '../components/PageTitle'
import {
    accessTokenFragment,
    AccessTokenNode,
    AccessTokenNodeProps,
    FilteredAccessTokenConnection,
} from '../settings/tokens/AccessTokenNode'
import { eventLogger } from '../tracking/eventLogger'
import { userURL } from '../user'

interface Props extends RouteComponentProps<{}> {
    authenticatedUser: GQL.IUser
}

interface State {}

/**
 * Displays a list of all access tokens on the site.
 */
export class SiteAdminTokensPage extends React.PureComponent<Props, State> {
    public state: State = {}

    private accessTokenUpdates = new Subject<void>()

    public componentDidMount(): void {
        eventLogger.logViewEvent('SiteAdminTokens')
    }

    public render(): JSX.Element | null {
        const nodeProps: Pick<AccessTokenNodeProps, 'showSubject' | 'onDidUpdate'> = {
            showSubject: true,
            onDidUpdate: this.onDidUpdateAccessToken,
        }

        return (
            <div className="user-settings-tokens-page">
                <PageTitle title="Access tokens - Admin" />
				<div className="d-flex justify-content-between align-items-center mt-3 mb-3">
					<h2 className="mb-0">Access tokens</h2>
					<Link
						className="btn btn-primary ml-2"
						to={`${userURL(this.props.authenticatedUser.username)}/account/tokens/new`}
					>
						<AddIcon className="icon-inline" /> Generate access token
					</Link>
				</div>
                <p>Tokens may be used to access the Sourcegraph API with the full privileges of the token's creator.</p>
                <FilteredAccessTokenConnection
					className="list-group list-group-flush mt-3"
                    noun="access token"
                    pluralNoun="access tokens"
                    queryConnection={this.queryAccessTokens}
                    nodeComponent={AccessTokenNode}
                    nodeComponentProps={nodeProps}
                    updates={this.accessTokenUpdates}
                    hideSearch={true}
                    noSummaryIfAllNodesVisible={true}
                    history={this.props.history}
                    location={this.props.location}
                />
            </div>
        )
    }

    private queryAccessTokens = (args: { first?: number }): Observable<GQL.IAccessTokenConnection> =>
        queryGraphQL(
            gql`
                query AccessTokens($first: Int) {
                    site {
                        accessTokens(first: $first) {
                            nodes {
                                ...AccessTokenFields
                            }
                            totalCount
                            pageInfo {
                                hasNextPage
                            }
                        }
                    }
                }
                ${accessTokenFragment}
            `,
            args
        ).pipe(
            map(({ data, errors }) => {
                if (!data || !data.site || !data.site.accessTokens || errors) {
                    throw createAggregateError(errors)
                }
                return data.site.accessTokens
            })
        )

    private onDidUpdateAccessToken = () => this.accessTokenUpdates.next()
}
