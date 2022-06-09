import React, { useCallback, useContext, useMemo, useState } from 'react'

import classNames from 'classnames'
import CheckCircleIcon from 'mdi-react/CheckCircleIcon'
import HelpCircleOutlineIcon from 'mdi-react/HelpCircleOutlineIcon'
import { CircularProgressbar } from 'react-circular-progressbar'
import { useHistory } from 'react-router-dom'

import { isExternalLink } from '@sourcegraph/common'
import { ModalVideo } from '@sourcegraph/search-ui'
import { Button, Icon, Link, Text, Tooltip } from '@sourcegraph/wildcard'

import { ItemPicker } from '../ItemPicker'

import { TourContext } from './context'
import { TourTaskType, TourLanguage, TourTaskStepType } from './types'
import { isLanguageRequired, getTourTaskStepActionValue } from './utils'

import styles from './Tour.module.scss'

type TourTaskProps = TourTaskType & {
    variant?: 'small'
}

/**
 * Tour task smart component. Handles all TourTaskStepType.type options.
 */
export const TourTask: React.FunctionComponent<React.PropsWithChildren<TourTaskProps>> = ({
    title,
    steps,
    completed,
    icon,
    variant,
    dataAttributes = {},
}) => {
    const [selectedStep, setSelectedStep] = useState<TourTaskStepType>()
    const [showLanguagePicker, setShowLanguagePicker] = useState(false)
    const { language, onLanguageSelect, onStepClick, onRestart } = useContext(TourContext)

    const handleLinkClick = useCallback(
        (event: React.MouseEvent<HTMLAnchorElement>, step: TourTaskStepType) => {
            onStepClick(step, language)
            if (isLanguageRequired(step) && !language) {
                event.preventDefault()
                setShowLanguagePicker(true)
                setSelectedStep(step)
            }
        },
        [language, onStepClick]
    )

    const handleVideoToggle = useCallback(
        (isOpen: boolean, step: TourTaskStepType) => {
            if (!isOpen) {
                onStepClick(step, language)
            }
        },
        [language, onStepClick]
    )

    const onLanguageClose = useCallback(() => setShowLanguagePicker(false), [])

    const history = useHistory()
    const handleLanguageSelect = useCallback(
        (language: TourLanguage) => {
            onLanguageSelect(language)
            setShowLanguagePicker(false)
            if (!selectedStep) {
                return
            }
            onStepClick(selectedStep, language)
            const url = getTourTaskStepActionValue(selectedStep, language)
            if (isExternalLink(url)) {
                window.open(url, '_blank')
            } else {
                history.push(url)
            }
        },
        [onStepClick, onLanguageSelect, selectedStep, history]
    )
    const attributes = useMemo(
        () =>
            Object.entries(dataAttributes).reduce(
                (result, [key, value]) => ({ ...result, [`data-${key}`]: value }),
                {}
            ),
        [dataAttributes]
    )

    if (showLanguagePicker) {
        return (
            <ItemPicker
                title="Please select a language:"
                className={classNames(variant !== 'small' && 'pl-2')}
                items={Object.values(TourLanguage)}
                onClose={onLanguageClose}
                onSelect={handleLanguageSelect}
            />
        )
    }

    const isMultiStep = steps.length > 1
    return (
        <div
            className={classNames(
                icon && [styles.task, variant === 'small' && styles.isSmall],
                !title && styles.noTitleTask
            )}
            {...attributes}
        >
            {icon && variant !== 'small' && <span className={styles.taskIcon}>{icon}</span>}
            <div className={classNames('flex-grow-1', variant !== 'small' && 'h-100 d-flex flex-column')}>
                {title && (
                    <div className="d-flex justify-content-between position-relative">
                        {icon && variant === 'small' && <span className={classNames(styles.taskIcon)}>{icon}</span>}
                        <Text className={styles.title}>{title}</Text>
                        {completed === 100 && (
                            <Icon as={CheckCircleIcon} size="sm" className="text-success" aria-label="Completed" />
                        )}
                        {typeof completed === 'number' && completed < 100 && (
                            <CircularProgressbar
                                className={styles.progressBar}
                                strokeWidth={10}
                                value={completed || 0}
                            />
                        )}
                    </div>
                )}
                <ul
                    className={classNames(
                        styles.stepList,
                        'm-0',
                        variant !== 'small' && 'flex-grow-1 d-flex flex-column',
                        isMultiStep && styles.isMultiStep
                    )}
                >
                    {steps.map(step => (
                        <li key={step.id} className={classNames(styles.stepListItem, 'd-flex align-items-center')}>
                            {step.action.type === 'link' && (
                                <Link
                                    className="flex-grow-1"
                                    to={getTourTaskStepActionValue(step, language)}
                                    onClick={event => handleLinkClick(event, step)}
                                >
                                    {step.label}
                                </Link>
                            )}
                            {step.action.type === 'new-tab-link' && (
                                <Link
                                    className={classNames(
                                        'flex-grow-1',
                                        step.action.variant === 'button-primary' && 'btn btn-primary'
                                    )}
                                    to={getTourTaskStepActionValue(step, language)}
                                    onClick={event => handleLinkClick(event, step)}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                >
                                    {step.label}
                                </Link>
                            )}
                            {step.action.type === 'restart' && (
                                <div className="flex-grow">
                                    <Text className="m-0">{step.label}</Text>
                                    <div className="d-flex flex-column">
                                        <Button
                                            variant="link"
                                            className="align-self-start text-left pl-0 font-weight-normal"
                                            onClick={() => onRestart(step)}
                                        >
                                            {step.action.value}
                                        </Button>
                                    </div>
                                </div>
                            )}
                            {step.action.type === 'video' && (
                                <ModalVideo
                                    id={step.id}
                                    showCaption={true}
                                    title={step.label}
                                    className="flex-grow-1"
                                    titleClassName="shadow-none text-left p-0 m-0"
                                    src={getTourTaskStepActionValue(step, language)}
                                    onToggle={isOpen => handleVideoToggle(isOpen, step)}
                                />
                            )}
                            {step.tooltip && (
                                <Tooltip content={step.tooltip}>
                                    <Icon
                                        as={HelpCircleOutlineIcon}
                                        size="sm"
                                        className={classNames('ml-1', styles.colorLink)}
                                        aria-label={step.tooltip}
                                    />
                                </Tooltip>
                            )}
                            {(isMultiStep || !title) && step.isCompleted && (
                                <Icon
                                    as={CheckCircleIcon}
                                    size="md"
                                    className="text-success"
                                    aria-label="Completed step"
                                />
                            )}
                        </li>
                    ))}
                </ul>
            </div>
        </div>
    )
}
