import React from 'react'
import * as GQL from '../../../shared/src/graphql/schema'
import { CopyableText } from '../components/CopyableText'
import { ExternalServiceKind } from '../../../shared/src/graphql-operations'

interface Props {
    externalService: Pick<GQL.IExternalService, 'kind' | 'webhookURL'>
}

export const SiteAdminExternalServiceWebhook: React.FunctionComponent<Props> = props => {
    const { kind, webhookURL } = props.externalService

    if (!webhookURL) {
        return <></>
    }

    let description = <p />

    switch (kind) {
        case ExternalServiceKind.BITBUCKETSERVER:
            description = (
                <p>
                    <a
                        href="https://docs.sourcegraph.com/admin/external_service/bitbucket_server#webhooks"
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        Webhooks
                    </a>{' '}
                    will be created automatically on the configured Bitbucket Server instance. In case you don't provide
                    an admin token,{' '}
                    <a
                        href="https://docs.sourcegraph.com/admin/external_service/bitbucket_server#manual-configuration"
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        follow the docs on how to set up webhooks manually
                    </a>
                    .
                    <br />
                    To set up another webhook manually, use the following URL:
                </p>
            )
            break

        case ExternalServiceKind.GITHUB:
            description = commonDescription('github')
            break

        case ExternalServiceKind.GITLAB:
            description = commonDescription('gitlab')
            break
    }

    return (
        <div className="alert alert-info">
            <h3>Campaign webhooks</h3>
            {description}
            <CopyableText className="mb-2" text={webhookURL} size={webhookURL.length} />
            <p className="mb-0">
                Note that only{' '}
                <a href="https://docs.sourcegraph.com/user/campaigns" target="_blank" rel="noopener noreferrer">
                    Campaigns
                </a>{' '}
                make use of this webhook. To enable webhooks to trigger repository updates on Sourcegraph,{' '}
                <a href="https://docs.sourcegraph.com/admin/repo/webhooks" target="_blank" rel="noopener noreferrer">
                    see the docs on how to use them
                </a>
                .
            </p>
        </div>
    )
}

function commonDescription(url: string): JSX.Element {
    return (
        <p>
            Point{' '}
            <a
                href={`https://docs.sourcegraph.com/admin/external_service/${url}#webhooks`}
                target="_blank"
                rel="noopener noreferrer"
            >
                webhooks
            </a>{' '}
            for this code host connection at the following URL:
        </p>
    )
}
