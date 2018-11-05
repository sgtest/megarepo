import * as H from 'history'
import InformationVariantIcon from 'mdi-react/InformationVariantIcon'
import * as React from 'react'
import { throwError } from 'rxjs'
import { catchError, map, tap } from 'rxjs/operators'
import * as GQL from '../../../backend/graphqlschema'
import { createThread } from '../../../discussions/backend'
import { eventLogger } from '../../../tracking/eventLogger'
import { asError } from '../../../util/errors'
import { parseHash } from '../../../util/url'
import { DiscussionsInput, TitleMode } from './DiscussionsInput'
import { DiscussionsNavbar } from './DiscussionsNavbar'

interface Props {
    repoID: GQL.ID
    repoPath: string
    commitID: string
    rev: string | undefined
    filePath: string
    history: H.History
    location: H.Location
}

interface State {
    title?: string
}

export class DiscussionsCreate extends React.PureComponent<Props, State> {
    constructor(props: Props) {
        super(props)
        this.state = {}
    }

    public render(): JSX.Element | null {
        return (
            <div className="discussions-create">
                <DiscussionsNavbar {...this.props} threadTitle={this.state.title} />
                <div className="discussions-create__content">
                    {this.state.title &&
                        this.state.title.length > 60 && (
                            <div className="alert alert-info p-1 mt-3 ml-3 mr-3 mb-0">
                                <small>
                                    <InformationVariantIcon className="icon-inline" />
                                    The first line of your message will become the title of your discussion. A good
                                    title is usually 50 characters or less.
                                </small>
                            </div>
                        )}
                    <DiscussionsInput
                        submitLabel="Create discussion"
                        titleMode={TitleMode.Implicit}
                        onTitleChange={this.onTitleChange}
                        onSubmit={this.onSubmit}
                        {...this.props}
                    />
                </div>
            </div>
        )
    }

    private onSubmit = (title: string, contents: string) => {
        eventLogger.log('CreatedDiscussion')

        const lpr = parseHash(window.location.hash)

        // lpr is one-based, discussions is zero-based (like LSP).
        // lpr endings are inclusive, discussions is exclusive (like LSP).
        const startLine = lpr.line ? lpr.line - 1 : 0
        const startCharacter = lpr.character ? lpr.character - 1 : 0
        const endLine = lpr.endLine ? lpr.endLine : startLine + 1
        const endCharacter = lpr.endCharacter || 0

        return createThread({
            title,
            contents,
            targetRepo: {
                repositoryID: this.props.repoID,
                path: this.props.filePath,
                branch: this.props.rev,
                revision: this.props.commitID,
                selection: {
                    startLine,
                    startCharacter,
                    endLine,
                    endCharacter,

                    // TODO(slimsag:discussions): Even though these fields are declared as
                    // nullable in the GraphQL schema ("lines: [String!]"), graphqlschema.ts
                    // not generate the proper nullable type, so we must cast to any.
                    //
                    // See https://github.com/sourcegraph/sourcegraph/issues/13098
                    linesBefore: null as any,
                    lines: null as any,
                    linesAfter: null as any,
                },
            },
        }).pipe(
            tap(thread => {
                const location = this.props.location
                const hash = new URLSearchParams(location.hash.slice('#'.length))
                hash.set('tab', 'discussions')
                hash.set('threadID', thread.id)
                // TODO(slimsag:discussions): ASAP: focus the new thread's range
                this.props.history.push(location.pathname + location.search + '#' + hash.toString())
            }),
            map(thread => undefined),
            catchError(e => throwError('Error creating thread: ' + asError(e).message))
        )
    }

    private onTitleChange = (newTitle: string) => this.setState({ title: newTitle })
}
