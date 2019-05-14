import ContentCopyIcon from 'mdi-react/ContentCopyIcon'
import DeleteIcon from 'mdi-react/DeleteIcon'
import PencilIcon from 'mdi-react/PencilIcon'
import * as React from 'react'
import { Subject, Subscription } from 'rxjs'
import { mapTo, startWith, switchMap, withLatestFrom } from 'rxjs/operators'
import * as GQL from '../../../../shared/src/graphql/schema'
import { SettingsCascadeProps } from '../../../../shared/src/settings/settings'
import { ThemeProps } from '../../theme'
import { eventLogger } from '../../tracking/eventLogger'
import { createSavedSearch, deleteSavedSearch } from '../backend'
import { SavedQueryRow } from './SavedQueryRow'
import { SavedQueryUpdateForm } from './SavedQueryUpdateForm'
interface Props extends SettingsCascadeProps, ThemeProps {
    authenticatedUser: GQL.IUser | null
    savedQuery: GQL.ISavedSearch
    onDidUpdate?: () => void
    onDidDuplicate?: () => void
    onDidDelete?: () => void
}

interface State {
    isEditing: boolean
    isSaving: boolean
    loading: boolean
    error?: Error
    approximateResultCount?: string
    sparkline?: number[]
    refreshedAt: number
    redirect: boolean
}

export class SavedQuery extends React.PureComponent<Props, State> {
    public state: State = { isEditing: false, isSaving: false, loading: true, refreshedAt: 0, redirect: false }

    private componentUpdates = new Subject<Props>()
    private refreshRequested = new Subject<GQL.ISavedSearch>()
    private duplicateRequested = new Subject<void>()
    private deleteRequested = new Subject<void>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        const propsChanges = this.componentUpdates.pipe(startWith(this.props))

        this.subscriptions.add(
            this.duplicateRequested
                .pipe(
                    withLatestFrom(propsChanges),
                    switchMap(([, props]) =>
                        createSavedSearch(
                            props.savedQuery.description,
                            props.savedQuery.query,
                            props.savedQuery.notify,
                            props.savedQuery.notifySlack,
                            props.savedQuery.userID,
                            props.savedQuery.orgID
                        )
                    ),
                    mapTo(void 0)
                )
                .subscribe(
                    () => {
                        if (this.props.onDidDuplicate) {
                            this.props.onDidDuplicate()
                        }
                        if (this.props.onDidUpdate) {
                            this.props.onDidUpdate()
                        }
                    },
                    err => {
                        console.error(err)
                    }
                )
        )

        this.subscriptions.add(
            this.deleteRequested
                .pipe(
                    withLatestFrom(propsChanges),
                    switchMap(([, props]) => deleteSavedSearch(props.savedQuery.id)),
                    mapTo(void 0)
                )
                .subscribe(
                    () => {
                        if (this.props.onDidDelete) {
                            this.props.onDidDelete()
                        }
                        if (this.props.onDidUpdate) {
                            this.props.onDidUpdate()
                        }
                    },
                    err => {
                        console.error(err)
                    }
                )
        )
    }

    public componentWillReceiveProps(newProps: Props): void {
        this.componentUpdates.next(newProps)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element {
        return (
            <SavedQueryRow
                query={this.props.savedQuery.query}
                description={this.props.savedQuery.description}
                className={this.state.isEditing ? 'editing' : ''}
                eventName="SavedQueryClick"
                isLightTheme={this.props.isLightTheme}
                actions={
                    <div className="saved-query-row__actions">
                        {!this.state.isEditing && (
                            <button className="btn btn-icon action" onClick={this.toggleEditing}>
                                <PencilIcon className="icon-inline" />
                                Edit
                            </button>
                        )}
                        {!this.state.isEditing && (
                            <button className="btn btn-icon action" onClick={this.duplicate}>
                                <ContentCopyIcon className="icon-inline" />
                                Duplicate
                            </button>
                        )}
                        <button className="btn btn-icon action" onClick={this.confirmDelete}>
                            <DeleteIcon className="icon-inline" />
                            Delete
                        </button>
                    </div>
                }
                form={
                    this.state.isEditing && (
                        <div className="saved-query-row__row">
                            <SavedQueryUpdateForm
                                authenticatedUser={this.props.authenticatedUser}
                                savedQuery={this.props.savedQuery}
                                onDidUpdate={this.onDidUpdateSavedQuery}
                                onDidCancel={this.toggleEditing}
                                settingsCascade={this.props.settingsCascade}
                            />
                        </div>
                    )
                }
            />
        )
    }

    private toggleEditing = (e?: React.MouseEvent<HTMLElement>) => {
        if (e) {
            e.stopPropagation()
            e.preventDefault()
        }
        eventLogger.log('SavedQueryToggleEditing', { queries: { editing: !this.state.isEditing } })
        this.setState(state => ({ isEditing: !state.isEditing }))
    }

    private onDidUpdateSavedQuery = () => {
        eventLogger.log('SavedQueryUpdated')
        this.setState({ isEditing: false, approximateResultCount: undefined, loading: true }, () => {
            this.refreshRequested.next()
            if (this.props.onDidUpdate) {
                this.props.onDidUpdate()
            }
        })
    }

    private duplicate = (e: React.MouseEvent<HTMLElement>) => {
        e.stopPropagation()
        e.preventDefault()
        this.duplicateRequested.next()
    }

    private confirmDelete = (e: React.MouseEvent<HTMLElement>) => {
        e.stopPropagation()
        e.preventDefault()
        if (window.confirm('Delete this saved query?')) {
            eventLogger.log('SavedQueryDeleted')
            this.deleteRequested.next()
        } else {
            eventLogger.log('SavedQueryDeletedCanceled')
        }
    }
}
