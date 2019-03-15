import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import H from 'history'
import CheckboxBlankCircleIcon from 'mdi-react/CheckboxBlankCircleIcon'
import CheckIcon from 'mdi-react/CheckIcon'
import * as React from 'react'
import { Link } from '../Link'
import { ActivationCompletionStatus, ActivationStep } from './Activation'

interface ActivationChecklistItemProps extends ActivationStep {
    done: boolean
    history: H.History
}

/**
 * A single item in the activation checklist.
 */
export class ActivationChecklistItem extends React.PureComponent<ActivationChecklistItemProps, {}> {
    private onClick = (e: React.MouseEvent<HTMLElement>) => {
        if (this.props.onClick) {
            this.props.onClick(e, this.props.history)
        }
    }
    public render(): JSX.Element {
        const checkboxElem = (
            <div className={'activation-item'}>
                {this.props.title}
                &nbsp;
                {this.props.done ? (
                    <CheckIcon className="icon-inline activation-item__checkbox--done" />
                ) : (
                    <CheckboxBlankCircleIcon className="icon-inline activation-item__checkbox--todo" />
                )}
            </div>
        )

        return (
            <div onClick={this.onClick} data-tooltip={this.props.detail}>
                {this.props.link ? (
                    <Link className={'activation-item__link'} {...this.props.link}>
                        {checkboxElem}
                    </Link>
                ) : (
                    <span className="activation-item__link">{checkboxElem}</span>
                )}
            </div>
        )
    }
}

export interface ActivationChecklistProps {
    history: H.History
    steps: ActivationStep[]
    completed?: ActivationCompletionStatus
}

/**
 * Renders an activation checklist.
 */
export class ActivationChecklist extends React.PureComponent<ActivationChecklistProps, {}> {
    public render(): JSX.Element {
        return (
            <div className="activation-checklist">
                {this.props.completed ? (
                    this.props.steps.map(step => (
                        <div key={step.id} className="activation-checklist__item">
                            <ActivationChecklistItem
                                {...step}
                                history={this.props.history}
                                done={(this.props.completed && this.props.completed[step.id]) || false}
                            />
                        </div>
                    ))
                ) : (
                    <div>
                        <LoadingSpinner className="icon-inline" />
                    </div>
                )}
            </div>
        )
    }
}
