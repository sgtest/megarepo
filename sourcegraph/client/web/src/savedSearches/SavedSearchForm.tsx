import React, { useMemo, useState } from 'react'
import { Omit } from 'utility-types'

import { Form } from '@sourcegraph/branded/src/components/Form'
import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { Container, PageHeader } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { ErrorAlert } from '../components/alerts'
import { NamespaceProps } from '../namespaces'

export interface SavedQueryFields {
    id: Scalars['ID']
    description: string
    query: string
    notify: boolean
    notifySlack: boolean
    slackWebhookURL: string | null
}

export interface SavedSearchFormProps extends NamespaceProps {
    authenticatedUser: AuthenticatedUser | null
    defaultValues?: Partial<SavedQueryFields>
    title?: string
    submitLabel: string
    onSubmit: (fields: Omit<SavedQueryFields, 'id'>) => void
    loading: boolean
    error?: any
}

export const SavedSearchForm: React.FunctionComponent<SavedSearchFormProps> = props => {
    const [values, setValues] = useState<Omit<SavedQueryFields, 'id'>>(() => ({
        description: props.defaultValues?.description || '',
        query: props.defaultValues?.query || '',
        notify: props.defaultValues?.notify || false,
        notifySlack: props.defaultValues?.notifySlack || false,
        slackWebhookURL: props.defaultValues?.slackWebhookURL || '',
    }))

    /**
     * Returns an input change handler that updates the SavedQueryFields in the component's state
     *
     * @param key The key of saved query fields that a change of this input should update
     */
    const createInputChangeHandler = (
        key: keyof SavedQueryFields
    ): React.FormEventHandler<HTMLInputElement> => event => {
        const { value, checked, type } = event.currentTarget
        setValues(values => ({
            ...values,
            [key]: type === 'checkbox' ? checked : value,
        }))
    }

    const handleSubmit = (event: React.FormEvent<HTMLFormElement>): void => {
        event.preventDefault()
        props.onSubmit(values)
    }

    /**
     * Tells if the query is unsupported for sending notifications.
     */
    const isUnsupportedNotifyQuery = useMemo((): boolean => {
        const notifying = values.notify || values.notifySlack
        return notifying && !values.query.includes('type:diff') && !values.query.includes('type:commit')
    }, [values])

    const { query, description, notify, notifySlack, slackWebhookURL } = values

    return (
        <div className="saved-search-form">
            <PageHeader
                path={[{ text: props.title }]}
                headingElement="h2"
                description="Get notifications when there are new results for specific search queries."
                className="mb-3"
            />
            <Form onSubmit={handleSubmit}>
                <Container className="mb-3">
                    <div className="form-group">
                        <label className="saved-search-form__label" htmlFor="saved-search-form-input-description">
                            Description
                        </label>
                        <input
                            id="saved-search-form-input-description"
                            type="text"
                            name="description"
                            className="form-control test-saved-search-form-input-description"
                            placeholder="Description"
                            required={true}
                            value={description}
                            onChange={createInputChangeHandler('description')}
                        />
                    </div>
                    <div className="form-group">
                        <label className="saved-search-form__label" htmlFor="saved-search-form-input-query">
                            Query
                        </label>
                        <input
                            id="saved-search-form-input-query"
                            type="text"
                            name="query"
                            className="form-control test-saved-search-form-input-query"
                            placeholder="Query"
                            required={true}
                            value={query}
                            onChange={createInputChangeHandler('query')}
                        />
                    </div>
                    <div className="form-group mb-0">
                        {/* Label is for visual benefit, input has more specific label attached */}
                        {/* eslint-disable-next-line jsx-a11y/label-has-associated-control */}
                        <label className="saved-search-form__label" id="saved-search-form-email-notifications">
                            Email notifications
                        </label>
                        <div aria-labelledby="saved-search-form-email-notifications">
                            <label>
                                <input
                                    type="checkbox"
                                    name="Notify owner"
                                    className="saved-search-form__checkbox"
                                    defaultChecked={notify}
                                    onChange={createInputChangeHandler('notify')}
                                />{' '}
                                <span>
                                    {props.namespace.__typename === 'Org'
                                        ? 'Send email notifications to all members of this organization'
                                        : props.namespace.__typename === 'User'
                                        ? 'Send email notifications to my email'
                                        : 'Email notifications'}
                                </span>
                            </label>
                        </div>
                    </div>
                    {notifySlack && slackWebhookURL && (
                        <div className="form-group mt-3 mb-0">
                            <label className="saved-search-form__label" htmlFor="saved-search-form-input-slack">
                                Slack notifications
                            </label>
                            <input
                                id="saved-search-form-input-slack"
                                type="text"
                                name="Slack webhook URL"
                                className="form-control"
                                value={slackWebhookURL}
                                disabled={true}
                                onChange={createInputChangeHandler('slackWebhookURL')}
                            />
                            <small>
                                Slack webhooks are deprecated and will be removed in a future Sourcegraph version.
                            </small>
                        </div>
                    )}
                    {isUnsupportedNotifyQuery && (
                        <div className="alert alert-warning mb-3">
                            <strong>Warning:</strong> non-commit searches do not currently support notifications.
                            Consider adding <code>type:diff</code> or <code>type:commit</code> to your query.
                        </div>
                    )}
                    {notify && !window.context.emailEnabled && !isUnsupportedNotifyQuery && (
                        <div className="alert alert-warning mb-3">
                            <strong>Warning:</strong> Sending emails is not currently configured on this Sourcegraph
                            server.{' '}
                            {props.authenticatedUser?.siteAdmin
                                ? 'Use the email.smtp site configuration setting to enable sending emails.'
                                : 'Contact your server admin for more information.'}
                        </div>
                    )}
                </Container>
                <button
                    type="submit"
                    disabled={props.loading}
                    className="btn btn-primary saved-search-form__submit-button test-saved-search-form-submit-button"
                >
                    {props.submitLabel}
                </button>
                {props.error && !props.loading && <ErrorAlert className="mb-3" error={props.error} />}
            </Form>
        </div>
    )
}
