import classNames from 'classnames'
import CloseIcon from 'mdi-react/CloseIcon'
import React from 'react'

import { Button, Card, Link } from '@sourcegraph/wildcard'

import styles from './CtaAlert.module.scss'

export interface CtaAlertProps {
    title: string
    description: string | React.ReactNode
    cta: {
        label: string
        href: string
        onClick?: () => void
    }
    icon: React.ReactNode
    className: string
    onClose: () => void
}

export const CtaAlert: React.FunctionComponent<CtaAlertProps> = props => (
    <Card
        className={classNames('my-2', 'd-flex', 'p-3', 'pr-5', 'flex-md-row', 'flex-column', props.className || null)}
    >
        <div className="mr-md-3">{props.icon}</div>
        <div className="flex-1 my-md-0 my-2">
            <div className={classNames('mb-1', styles.ctaTitle)}>
                <strong>{props.title}</strong>
            </div>
            <div className={classNames('text-muted', 'mb-2', styles.ctaDescription)}>{props.description}</div>
            <Button to={props.cta.href} onClick={props.cta.onClick} variant="primary" as={Link}>
                {props.cta.label}
            </Button>
        </div>
        <CloseIcon
            className="icon-inline position-absolute cursor-pointer"
            style={{ top: '1rem', right: '1rem' }}
            onClick={props.onClose}
        />
    </Card>
)
