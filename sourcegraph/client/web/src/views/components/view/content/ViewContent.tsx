import classNames from 'classnames'
import { isObject } from 'lodash'
import React, { useEffect, useRef } from 'react'
import { View, MarkupContent } from 'sourcegraph'

import { MarkupKind } from '@sourcegraph/extension-api-classes'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { ErrorLike } from '@sourcegraph/shared/src/codeintellify/errors'
import { Markdown } from '@sourcegraph/shared/src/components/Markdown'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { renderMarkdown } from '@sourcegraph/shared/src/util/markdown'
import { hasProperty } from '@sourcegraph/shared/src/util/types'

import { ErrorAlert } from '../../../../components/alerts'

import { ChartViewContent } from './chart-view-content/ChartViewContent'
import styles from './ViewContent.module.scss'

const isMarkupContent = (input: unknown): input is MarkupContent =>
    isObject(input) && hasProperty('value')(input) && typeof input.value === 'string'

export interface ViewContentProps extends TelemetryProps {
    content: View['content']
    viewID: string

    /** To get container to track hovers for pings */
    containerClassName?: string

    /** Optionally display an alert overlay */
    alert?: React.ReactNode
}

/**
 * Renders the content sections of the view. It supports markdown and different types
 * of chart views.
 *
 * Support for non-MarkupContent elements is experimental and subject to change or removal
 * without notice.
 */
export const ViewContent: React.FunctionComponent<ViewContentProps> = props => {
    const { content, viewID, containerClassName, alert, telemetryService } = props

    // Track user intent to interact with extension-contributed views
    const viewContentReference = useRef<HTMLDivElement>(null)

    // TODO Move this tracking logic out of this shared view component
    useEffect(() => {
        let viewContentElement = viewContentReference.current

        let timeoutID: number | undefined

        function onMouseEnter(): void {
            // Set timer to increase confidence that the user meant to interact with the
            // view, as opposed to accidentally moving past it. If the mouse leaves
            // the view quickly, clear the timeout for logging the event
            timeoutID = window.setTimeout(() => {
                telemetryService.log(
                    'InsightHover',
                    { insightType: viewID.split('.')[0] },
                    { insightType: viewID.split('.')[0] }
                )
            }, 500)

            viewContentElement?.addEventListener('mouseleave', onMouseLeave)
        }

        function onMouseLeave(): void {
            clearTimeout(timeoutID)
            viewContentElement?.removeEventListener('mouseleave', onMouseLeave)
        }

        // If containerClassName is specified, the element with this class is the element
        // that embodies the view in the eyes of the user. e.g. InsightsViewGrid
        if (containerClassName) {
            viewContentElement = viewContentElement?.closest(`.${containerClassName}`) as HTMLDivElement
        }

        viewContentElement?.addEventListener('mouseenter', onMouseEnter)

        return () => {
            viewContentElement?.removeEventListener('mouseenter', onMouseEnter)
            viewContentElement?.removeEventListener('mouseleave', onMouseLeave)
            clearTimeout(timeoutID)
        }
    }, [viewID, containerClassName, telemetryService])

    return (
        <div className={styles.viewContent} ref={viewContentReference}>
            {content.map((content, index) =>
                isMarkupContent(content) ? (
                    <React.Fragment key={index}>
                        {content.kind === MarkupKind.Markdown || !content.kind ? (
                            <Markdown
                                className={classNames('mb-1', styles.markdown)}
                                dangerousInnerHTML={renderMarkdown(content.value)}
                            />
                        ) : (
                            content.value
                        )}
                    </React.Fragment>
                ) : 'chart' in content ? (
                    <React.Fragment key={index}>
                        {alert && <div className={styles.viewContentAlertOverlay}>{alert}</div>}
                        <ChartViewContent
                            content={content}
                            viewID={viewID}
                            telemetryService={props.telemetryService}
                            className={styles.chart}
                        />
                    </React.Fragment>
                ) : null
            )}
        </div>
    )
}

export interface ViewErrorContentProps {
    title: string
    error: ErrorLike
}

export const ViewErrorContent: React.FunctionComponent<ViewErrorContentProps> = props => {
    const { error, title, children } = props

    return (
        <div className="h-100 w-100 d-flex flex-column">
            {children || <ErrorAlert data-testid={`${title} view error`} className="m-0" error={error} />}
        </div>
    )
}

export interface ViewLoadingContentProps {
    text: string
}

export const ViewLoadingContent: React.FunctionComponent<ViewLoadingContentProps> = props => {
    const { text } = props

    return (
        <div className="h-100 w-100 d-flex flex-column">
            <span className="flex-grow-1 d-flex flex-column align-items-center justify-content-center">
                <LoadingSpinner /> {text}
            </span>
        </div>
    )
}

export const ViewBannerContent: React.FunctionComponent = ({ children }) => (
    <div className="h-100 w-100 d-flex flex-column justify-content-center align-items-center">{children}</div>
)
