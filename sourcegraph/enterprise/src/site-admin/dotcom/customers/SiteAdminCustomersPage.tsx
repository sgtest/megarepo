import { gql, queryGraphQL } from '@sourcegraph/webapp/dist/backend/graphql'
import * as GQL from '@sourcegraph/webapp/dist/backend/graphqlschema'
import { FilteredConnection } from '@sourcegraph/webapp/dist/components/FilteredConnection'
import { PageTitle } from '@sourcegraph/webapp/dist/components/PageTitle'
import { eventLogger } from '@sourcegraph/webapp/dist/tracking/eventLogger'
import { userURL } from '@sourcegraph/webapp/dist/user'
import { createAggregateError } from '@sourcegraph/webapp/dist/util/errors'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Observable, Subject, Subscription } from 'rxjs'
import { map } from 'rxjs/operators'
import { AccountName } from '../../../dotcom/productSubscriptions/AccountName'
import { SiteAdminCustomerBillingLink } from './SiteAdminCustomerBillingLink'

const siteAdminCustomerFragment = gql`
    fragment CustomerFields on User {
        id
        username
        displayName
        urlForSiteAdminBilling
    }
`

interface SiteAdminCustomerNodeProps {
    node: Pick<GQL.IUser, 'id' | 'username' | 'displayName' | 'urlForSiteAdminBilling'>
    onDidUpdate: () => void
}

/**
 * Displays a customer in a connection in the site admin area.
 */
class SiteAdminCustomerNode extends React.PureComponent<SiteAdminCustomerNodeProps> {
    public render(): JSX.Element | null {
        return (
            <li className="list-group-item py-2">
                <div className="d-flex align-items-center justify-content-between">
                    <span className="mr-3">
                        <AccountName
                            account={this.props.node}
                            link={`${userURL(this.props.node.username)}/subscriptions`}
                        />
                    </span>
                    <SiteAdminCustomerBillingLink customer={this.props.node} onDidUpdate={this.props.onDidUpdate} />
                </div>
            </li>
        )
    }
}

interface Props extends RouteComponentProps<{}> {}

class FilteredSiteAdminCustomerConnection extends FilteredConnection<
    Pick<GQL.IUser, 'id' | 'username' | 'displayName' | 'urlForSiteAdminBilling'>,
    Pick<SiteAdminCustomerNodeProps, Exclude<keyof SiteAdminCustomerNodeProps, 'node'>>
> {}

/**
 * Displays a list of customers associated with user accounts on Sourcegraph.com.
 */
export class SiteAdminProductCustomersPage extends React.Component<Props> {
    private subscriptions = new Subscription()
    private updates = new Subject<void>()

    public componentDidMount(): void {
        eventLogger.logViewEvent('SiteAdminProductCustomers')
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        const nodeProps: Pick<SiteAdminCustomerNodeProps, Exclude<keyof SiteAdminCustomerNodeProps, 'node'>> = {
            onDidUpdate: this.onUserUpdate,
        }

        return (
            <div className="site-admin-customers-page">
                <PageTitle title="Customers" />
                <h2>Customers</h2>
                <p>User accounts may be linked to a customer on the billing system.</p>
                <FilteredSiteAdminCustomerConnection
                    className="list-group list-group-flush mt-3"
                    noun="customer"
                    pluralNoun="customers"
                    queryConnection={this.queryCustomers}
                    nodeComponent={SiteAdminCustomerNode}
                    nodeComponentProps={nodeProps}
                    noSummaryIfAllNodesVisible={true}
                    updates={this.updates}
                    history={this.props.history}
                    location={this.props.location}
                />
            </div>
        )
    }

    private queryCustomers = (args: { first?: number; query?: string }): Observable<GQL.IUserConnection> =>
        queryGraphQL(
            gql`
                query Customers($first: Int, $query: String) {
                    users(first: $first, query: $query) {
                        nodes {
                            ...CustomerFields
                        }
                        totalCount
                        pageInfo {
                            hasNextPage
                        }
                    }
                }
                ${siteAdminCustomerFragment}
            `,
            {
                first: args.first,
                query: args.query,
            } as GQL.IUsersOnQueryArguments
        ).pipe(
            map(({ data, errors }) => {
                if (!data || !data.users || (errors && errors.length > 0)) {
                    throw createAggregateError(errors)
                }
                return data.users
            })
        )

    private onUserUpdate = () => this.updates.next()
}
