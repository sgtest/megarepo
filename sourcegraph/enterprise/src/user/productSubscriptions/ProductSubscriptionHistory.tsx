import * as GQL from '@sourcegraph/webapp/dist/backend/graphqlschema'
import { LinkOrSpan } from '@sourcegraph/webapp/dist/components/LinkOrSpan'
import format from 'date-fns/format'
import ExternalLinkIcon from 'mdi-react/ExternalLinkIcon'
import React from 'react'

export const ProductSubscriptionHistory: React.SFC<{
    productSubscription: Pick<GQL.IProductSubscription, 'events'>
}> = ({ productSubscription }) =>
    productSubscription.events.length > 0 ? (
        <table className="table mb-0">
            <thead>
                <tr>
                    <th>Date</th>
                    <th>Description</th>
                </tr>
            </thead>
            <tbody>
                {productSubscription.events.map(event => (
                    <tr key={event.id}>
                        <td className="text-nowrap">
                            <span data-tooltip={format(event.date, 'PPpp')}>{format(event.date, 'yyyy-MM-dd')}</span>
                        </td>
                        <td className="w-100">
                            <LinkOrSpan to={event.url} target="_blank">
                                {event.title}
                                {event.url && <ExternalLinkIcon className="icon-inline ml-1" />}
                            </LinkOrSpan>
                            {event.description && <small className="d-block text-muted">{event.description}</small>}
                        </td>
                    </tr>
                ))}
            </tbody>
        </table>
    ) : (
        <div className="card-body">
            <span className="text-muted">No events</span>
        </div>
    )
