import * as H from 'history'
import * as React from 'react'
import { Link } from 'react-router-dom'
import { Observable } from 'rxjs'
import { ChatIcon } from '../../../shared/src/components/icons'
import * as GQL from '../../../shared/src/graphql/schema'
import { FilteredConnection, FilteredConnectionQueryArgs } from '../components/FilteredConnection'
import { Timestamp } from '../components/time/Timestamp'
import { fetchDiscussionThreads } from './backend'

interface DiscussionNodeProps {
    node: Pick<
        GQL.IDiscussionThread,
        'idWithoutKind' | 'title' | 'author' | 'inlineURL' | 'comments' | 'createdAt' | 'target'
    >
    location: H.Location
    withRepo?: boolean
}

const DiscussionNode: React.FunctionComponent<DiscussionNodeProps> = ({ node, location, withRepo }) => {
    const currentURL = location.pathname + location.search + location.hash

    // TODO(slimsag:discussions): future: Improve rendering of discussions when there is no inline URL
    const inlineURL = node.inlineURL || ''

    return (
        <li className={'discussions-list__row' + (currentURL === inlineURL ? ' discussions-list__row--active' : '')}>
            <div className="d-flex align-items-center justify-content-between">
                <h3 className="discussions-list__row-title mb-0">
                    <Link to={inlineURL}>{node.title}</Link>
                </h3>
                <Link to={inlineURL} className="text-muted">
                    <ChatIcon className="icon-inline mr-1" />
                    {node.comments.totalCount}
                </Link>
            </div>
            <div className="text-muted">
                #{node.idWithoutKind} created <Timestamp date={node.createdAt} /> by{' '}
                <Link to={`/users/${node.author.username}`} data-tooltip={node.author.displayName}>
                    {node.author.username}
                </Link>{' '}
                {withRepo && (
                    <>
                        in <Link to={node.target.repository.name}>{node.target.repository.name}</Link>
                    </>
                )}
            </div>
        </li>
    )
}

class FilteredDiscussionsConnection extends FilteredConnection<
    DiscussionNodeProps['node'],
    Pick<DiscussionNodeProps, 'location'>
> {}

interface Props {
    repoID: GQL.ID | undefined
    rev: string | undefined
    filePath: string | undefined
    history: H.History
    location: H.Location

    autoFocus?: boolean
    defaultFirst?: number
    hideSearch?: boolean
    noun?: string
    pluralNoun?: string
    noFlex?: boolean
    withRepo?: boolean
    compact: boolean
}

export class DiscussionsList extends React.PureComponent<Props> {
    public render(): JSX.Element | null {
        const nodeComponentProps: Pick<DiscussionNodeProps, 'location' | 'withRepo'> = {
            location: this.props.location,
            withRepo: this.props.withRepo,
        }
        return (
            <FilteredDiscussionsConnection
                className={'discussions-list' + this.props.noFlex ? 'discussions-list--no-flex' : ''}
                autoFocus={this.props.autoFocus !== undefined ? this.props.autoFocus : true}
                compact={this.props.compact}
                noun={this.props.noun || 'discussion'}
                pluralNoun={this.props.pluralNoun || 'discussions'}
                queryConnection={this.fetchThreads}
                nodeComponent={DiscussionNode}
                nodeComponentProps={nodeComponentProps}
                updateOnChange={`${String(this.props.repoID)}:${String(this.props.rev)}:${String(this.props.filePath)}`}
                defaultFirst={this.props.defaultFirst || 100}
                hideSearch={this.props.hideSearch}
                useURLQuery={false}
                history={this.props.history}
                location={this.props.location}
            />
        )
    }

    private fetchThreads = (args: FilteredConnectionQueryArgs): Observable<GQL.IDiscussionThreadConnection> =>
        fetchDiscussionThreads({
            ...args,
            targetRepositoryID: this.props.repoID,
            targetRepositoryPath: this.props.filePath,
        })
}
