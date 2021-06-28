import * as H from 'history'
import React from 'react'
import { Link } from 'react-router-dom'

import { ResolvedRevisionSpec, RevisionSpec } from '@sourcegraph/shared/src/util/url'

import { useScrollToLocationHash } from '../../components/useScrollToLocationHash'
import { RepositoryFields } from '../../graphql-operations'
import { toDocumentationURL } from '../../util/url'

import { GQLDocumentationNode } from './DocumentationNode'

interface Props extends Partial<RevisionSpec>, ResolvedRevisionSpec {
    repo: RepositoryFields

    history: H.History
    location: H.Location
    node: GQLDocumentationNode
    depth: number
    pagePathID: string
}

export const DocumentationIndexNode: React.FunctionComponent<Props> = ({ node, depth, ...props }) => {
    useScrollToLocationHash(props.location)
    const repoRevision = {
        repoName: props.repo.name,
        revision: props.revision || '',
    }
    const hashIndex = node.pathID.indexOf('#')
    const hash = hashIndex ? node.pathID.slice(hashIndex + '#'.length) : ''
    const thisPage = toDocumentationURL({ ...repoRevision, pathID: node.pathID })

    return (
        <div className="documentation-index-node">
            <Link id={'index-' + hash} to={thisPage} className="text-nowrap">
                {node.label.value}
            </Link>

            <ul className="pl-3">
                {node.children?.map((child, index) =>
                    child.pathID ? (
                        <li key={`${depth}-${index}`}>
                            <Link to={toDocumentationURL({ ...repoRevision, pathID: child.pathID })}>
                                {child.pathID}
                            </Link>
                        </li>
                    ) : (
                        <DocumentationIndexNode
                            key={`${depth}-${index}`}
                            {...props}
                            node={child.node!}
                            depth={depth + 1}
                        />
                    )
                )}
            </ul>
        </div>
    )
}
