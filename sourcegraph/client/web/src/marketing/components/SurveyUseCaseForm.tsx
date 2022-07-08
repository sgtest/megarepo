import React from 'react'

import classNames from 'classnames'

import { FlexTextArea, H4, Input } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../auth'
import { UseCaseFeedbackModifiers, UseCaseFeedbackState } from '../toast/SurveyUseCaseToast'

import styles from './SurveyUseCaseForm.module.scss'

interface SurveyUseCaseFormProps extends UseCaseFeedbackState, UseCaseFeedbackModifiers {
    formLabelClassName?: string
    className?: string
    authenticatedUser?: AuthenticatedUser | null
}

export const SurveyUseCaseForm: React.FunctionComponent<SurveyUseCaseFormProps> = ({
    otherUseCase,
    onChangeOtherUseCase,
    better,
    onChangeBetter,
    email,
    onChangeEmail,
    formLabelClassName,
    className,
    authenticatedUser,
}) => (
    <div className={classNames('mb-2', className)}>
        <FlexTextArea
            minRows={2}
            maxRows={6}
            containerClassName="mt-3"
            label={
                <H4 as="span" className={classNames('d-flex', styles.title, formLabelClassName)}>
                    What do you use Sourcegraph for?
                </H4>
            }
            onChange={event => onChangeOtherUseCase(event.target.value)}
            value={otherUseCase}
        />
        <FlexTextArea
            minRows={2}
            maxRows={6}
            containerClassName="mt-3"
            label={
                <H4 as="span" className={classNames('d-flex', styles.title, formLabelClassName)}>
                    What would make Sourcegraph better?
                </H4>
            }
            onChange={event => onChangeBetter(event.target.value)}
            value={better}
        />
        {!authenticatedUser && (
            <Input
                className="mt-3"
                label={
                    <H4 as="span" className={classNames('d-flex', styles.title, formLabelClassName)}>
                        What is your email?
                    </H4>
                }
                onChange={event => onChangeEmail(event.target.value)}
                value={email}
                type="email"
                autoComplete="email"
                name="email"
            />
        )}
    </div>
)
