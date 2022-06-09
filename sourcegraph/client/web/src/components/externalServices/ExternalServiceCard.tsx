import React from 'react'

import classNames from 'classnames'
import AccountIcon from 'mdi-react/AccountIcon'
import ChevronRightIcon from 'mdi-react/ChevronRightIcon'

import { Icon, Link, H3, Text } from '@sourcegraph/wildcard'

import { ExternalServiceFields, ExternalServiceKind } from '../../graphql-operations'

interface ExternalServiceCardProps {
    /**
     * Title to show in the external service "button"
     */
    title: string

    /**
     * Icon to show in the external service "button"
     */
    icon: React.ComponentType<React.PropsWithChildren<{ className?: string }>>

    /**
     * A short description that will appear in the external service "button" under the title
     */
    shortDescription?: string

    kind: ExternalServiceKind

    namespace?: ExternalServiceFields['namespace']

    to?: string
    className?: string
}

export const ExternalServiceCard: React.FunctionComponent<React.PropsWithChildren<ExternalServiceCardProps>> = ({
    title,
    icon: CardIcon,
    shortDescription,
    to,
    kind,
    namespace,
    className = '',
}) => {
    const children = (
        <div className={classNames('p-3 d-flex align-items-start border', className)}>
            <Icon className="h3 mb-0 mr-3" as={CardIcon} aria-hidden={true} />
            <div className="flex-1">
                <H3 className={shortDescription ? 'mb-0' : 'mt-1 mb-0'}>
                    {title}
                    {namespace && (
                        <small>
                            {' '}
                            by
                            <Icon as={AccountIcon} aria-hidden={true} />
                            <Link to={namespace.url}>{namespace.namespaceName}</Link>
                        </small>
                    )}
                </H3>
                {shortDescription && <Text className="mb-0 text-muted">{shortDescription}</Text>}
            </div>
            {to && <ChevronRightIcon className="align-self-center" />}
        </div>
    )
    return to ? (
        <Link
            className="d-block text-left text-body text-decoration-none"
            to={to}
            data-test-external-service-card-link={kind}
        >
            {children}
        </Link>
    ) : (
        children
    )
}
