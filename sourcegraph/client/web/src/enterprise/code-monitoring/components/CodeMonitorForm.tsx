import React, { useCallback, useMemo, useState } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import { isEqual } from 'lodash'
import { Observable } from 'rxjs'
import { mergeMap, startWith, catchError, tap, filter } from 'rxjs/operators'

import { Form } from '@sourcegraph/branded/src/components/Form'
import { Toggle } from '@sourcegraph/branded/src/components/Toggle'
import { asError, isErrorLike } from '@sourcegraph/common'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { Container, Button, useEventObservable, Alert, Link, Select } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../../auth'
import { CodeMonitorFields } from '../../../graphql-operations'
import { deleteCodeMonitor as _deleteCodeMonitor } from '../backend'

import { DeleteMonitorModal } from './DeleteMonitorModal'
import { FormActionArea } from './FormActionArea'
import { FormTriggerArea } from './FormTriggerArea'

import styles from './CodeMonitorForm.module.scss'

export interface CodeMonitorFormProps extends ThemeProps {
    history: H.History
    location: H.Location
    authenticatedUser: AuthenticatedUser
    /**
     * A function that takes in a code monitor and emits an Observable with all or some
     * of the CodeMonitorFields when the form is submitted.
     */
    onSubmit: (codeMonitor: CodeMonitorFields) => Observable<Partial<CodeMonitorFields>>
    /* The text for the submit button. */
    submitButtonLabel: string
    /* A code monitor to initialize the form with. */
    codeMonitor?: CodeMonitorFields
    /* Whether to show the delete button */
    showDeleteButton?: boolean
    /* Optional trigger query to pre-populate the trigger form */
    triggerQuery?: string
    /* Optional description to pre-populate the name */
    description?: string

    deleteCodeMonitor?: typeof _deleteCodeMonitor

    isSourcegraphDotCom: boolean
}

interface FormCompletionSteps {
    triggerCompleted: boolean
    actionCompleted: boolean
}

export const CodeMonitorForm: React.FunctionComponent<CodeMonitorFormProps> = ({
    authenticatedUser,
    onSubmit,
    history,
    submitButtonLabel,
    codeMonitor,
    showDeleteButton,
    deleteCodeMonitor = _deleteCodeMonitor,
    triggerQuery,
    description,
    isLightTheme,
    isSourcegraphDotCom,
}) => {
    const LOADING = 'loading' as const

    const [currentCodeMonitorState, setCodeMonitor] = useState<CodeMonitorFields>(
        codeMonitor ?? {
            id: '',
            description: description ?? '',
            enabled: true,
            trigger: { id: '', query: triggerQuery ?? '' },
            actions: {
                nodes: [],
            },
        }
    )

    const [formCompletion, setFormCompletion] = useState<FormCompletionSteps>({
        triggerCompleted: currentCodeMonitorState.trigger.query.length > 0,
        actionCompleted: currentCodeMonitorState.actions.nodes.length > 0,
    })
    const setTriggerCompleted = useCallback((complete: boolean) => {
        setFormCompletion(previousState => ({ ...previousState, triggerCompleted: complete }))
    }, [])
    const setActionsCompleted = useCallback((complete: boolean) => {
        setFormCompletion(previousState => ({ ...previousState, actionCompleted: complete }))
    }, [])

    const onNameChange = useCallback(
        (description: string): void => setCodeMonitor(codeMonitor => ({ ...codeMonitor, description })),
        []
    )
    const onQueryChange = useCallback(
        (query: string): void =>
            setCodeMonitor(codeMonitor => ({ ...codeMonitor, trigger: { ...codeMonitor.trigger, query } })),
        []
    )
    const onEnabledChange = useCallback(
        (enabled: boolean): void => setCodeMonitor(codeMonitor => ({ ...codeMonitor, enabled })),
        []
    )
    const onActionsChange = useCallback(
        (actions: CodeMonitorFields['actions']): void => setCodeMonitor(codeMonitor => ({ ...codeMonitor, actions })),
        []
    )

    const [requestOnSubmit, codeMonitorOrError] = useEventObservable(
        useCallback(
            (submit: Observable<React.FormEvent<HTMLFormElement>>) =>
                submit.pipe(
                    tap(event => event.preventDefault()),
                    filter(() => formCompletion.actionCompleted && formCompletion.triggerCompleted),
                    mergeMap(() =>
                        onSubmit(currentCodeMonitorState).pipe(
                            startWith(LOADING),
                            catchError(error => [asError(error)]),
                            tap(successOrError => {
                                if (!isErrorLike(successOrError) && successOrError !== LOADING) {
                                    history.push('/code-monitoring')
                                }
                            })
                        )
                    )
                ),
            [onSubmit, currentCodeMonitorState, history, formCompletion]
        )
    )

    const initialCodeMonitor = useMemo(() => codeMonitor, [codeMonitor])

    // Determine whether the form has changed. If there was no intial state (i.e. we're creating a monitor), always return
    // true.
    const hasChangedFields = useMemo(
        () => (codeMonitor ? !isEqual(initialCodeMonitor, currentCodeMonitorState) : true),
        [initialCodeMonitor, codeMonitor, currentCodeMonitorState]
    )

    const onCancel = useCallback(() => {
        if (hasChangedFields) {
            if (window.confirm('Leave page? All unsaved changes will be lost.')) {
                history.push('/code-monitoring')
            }
        } else {
            history.push('/code-monitoring')
        }
    }, [history, hasChangedFields])

    const [showDeleteModal, setShowDeleteModal] = useState(false)

    const toggleDeleteModal = useCallback(() => setShowDeleteModal(show => !show), [setShowDeleteModal])

    return (
        <>
            <Form className="my-4 pb-5" data-testid="monitor-form" onSubmit={requestOnSubmit}>
                <Container className="mb-3">
                    <div className="form-group">
                        <label htmlFor="code-monitor-form-name">Name</label>
                        <input
                            id="code-monitor-form-name"
                            type="text"
                            className="form-control mb-2 test-name-input"
                            data-testid="name-input"
                            required={true}
                            onChange={event => {
                                onNameChange(event.target.value)
                            }}
                            value={currentCodeMonitorState.description}
                            autoFocus={true}
                            spellCheck={false}
                        />
                        <small className="text-muted">
                            Give it a short, descriptive name to reference events on Sourcegraph and in notifications.
                            Do not include{' '}
                            <Link
                                to="/help/code_monitoring/explanations/best_practices#do-not-include-confidential-information-in-monitor-names"
                                target="_blank"
                                rel="noopener"
                            >
                                confidential information
                            </Link>
                            .
                        </small>
                    </div>

                    <Select
                        label="Owner"
                        className="w-100"
                        aria-label="Owner"
                        selectClassName={classNames('mb-2 w-auto', styles.ownerDropdown)}
                        disabled={true}
                        message="Event history and configuration will not be shared. Code monitoring currently only supports individual owners."
                    >
                        <option value={authenticatedUser.displayName || authenticatedUser.username}>
                            {authenticatedUser.username}
                        </option>
                    </Select>

                    <hr className={classNames('my-3', styles.horizontalRule)} />
                    <div className="mb-4">
                        <FormTriggerArea
                            query={currentCodeMonitorState.trigger.query}
                            onQueryChange={onQueryChange}
                            triggerCompleted={formCompletion.triggerCompleted}
                            setTriggerCompleted={setTriggerCompleted}
                            startExpanded={!!triggerQuery}
                            cardBtnClassName={styles.cardButton}
                            cardLinkClassName={styles.cardLink}
                            cardClassName={styles.card}
                            isLightTheme={isLightTheme}
                            isSourcegraphDotCom={isSourcegraphDotCom}
                        />
                    </div>
                    {/*
                        a11y-ignore
                        Rule: "color-contrast" (Elements must have sufficient color contrast)
                        GitHub issue: https://github.com/sourcegraph/sourcegraph/issues/33343
                    */}
                    <div
                        className={classNames(
                            !formCompletion.triggerCompleted && styles.actionsDisabled,
                            'a11y-ignore'
                        )}
                    >
                        <FormActionArea
                            actions={currentCodeMonitorState.actions}
                            setActionsCompleted={setActionsCompleted}
                            actionsCompleted={formCompletion.actionCompleted}
                            authenticatedUser={authenticatedUser}
                            disabled={!formCompletion.triggerCompleted}
                            onActionsChange={onActionsChange}
                            monitorName={currentCodeMonitorState.description}
                        />
                    </div>
                    <hr className={classNames('my-3', styles.horizontalRule)} />
                    <div>
                        <div className="d-flex">
                            <div>
                                <Toggle
                                    title="Active"
                                    value={currentCodeMonitorState.enabled}
                                    onToggle={onEnabledChange}
                                    className="mr-2"
                                    aria-describedby="code-monitor-form-toggle-description"
                                />{' '}
                            </div>
                            <div className="flex-column" id="code-monitor-form-toggle-description">
                                <div>{currentCodeMonitorState.enabled ? 'Active' : 'Inactive'}</div>
                                <div className="text-muted">
                                    {currentCodeMonitorState.enabled
                                        ? 'Code monitor will watch for the trigger and run actions in response'
                                        : 'Code monitor will not respond to trigger events'}
                                </div>
                            </div>
                        </div>
                    </div>
                </Container>
                <div>
                    <div className="d-flex justify-content-between my-4">
                        <div>
                            <Button
                                type="submit"
                                disabled={
                                    !formCompletion.actionCompleted ||
                                    !formCompletion.triggerCompleted ||
                                    codeMonitorOrError === LOADING ||
                                    !hasChangedFields
                                }
                                data-testid="submit-monitor"
                                className="mr-2 test-submit-monitor"
                                variant="primary"
                            >
                                {submitButtonLabel}
                            </Button>
                            <Button onClick={onCancel} data-testid="cancel-monitor" variant="secondary">
                                Cancel
                            </Button>
                        </div>
                        {showDeleteButton && (
                            <div>
                                <Button
                                    onClick={toggleDeleteModal}
                                    data-testid="delete-monitor"
                                    outline={true}
                                    variant="danger"
                                >
                                    Delete
                                </Button>
                            </div>
                        )}
                    </div>
                    {isErrorLike(codeMonitorOrError) && (
                        <Alert variant="danger">Failed to create monitor: {codeMonitorOrError.message}</Alert>
                    )}
                </div>
            </Form>
            {showDeleteButton && (
                <DeleteMonitorModal
                    isOpen={showDeleteModal}
                    deleteCodeMonitor={deleteCodeMonitor}
                    history={history}
                    codeMonitor={codeMonitor}
                    toggleDeleteModal={toggleDeleteModal}
                />
            )}
        </>
    )
}
