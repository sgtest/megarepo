import React, { ReactNode } from 'react'

import * as TooltipPrimitive from '@radix-ui/react-tooltip'
import classNames from 'classnames'

import styles from './Tooltip.module.scss'

interface TooltipProps {
    /** A single child element that will trigger the Tooltip to open on hover. */
    children: ReactNode
    /** The text that will be displayed in the Tooltip. If `null`, no Tooltip will be rendered, allowing for Tooltips to be shown conditionally. */
    content: string | null
    /** The open state of the tooltip when it is initially rendered. Defaults to `false`. */
    defaultOpen?: boolean
    /** The preferred side of the trigger to render against when open. Will be reversed if a collision is detected. Defaults to `right`. */
    placement?: TooltipPrimitive.TooltipContentProps['side']
    /** Class name to apply to the wrapping span */
    className?: string
}

/** Arrow width in pixels */
const TOOLTIP_ARROW_WIDTH = 14
/** Arrow height in pixel */
const TOOLTIP_ARROW_HEIGHT = 6

// Handling the onPointerDownOutside event and preventing the default behavior allows us to keep the Tooltip content open
// even if the trigger <span> was clicked; this allows buttons to be clicked and text to be selected without dismissing content.
// Reference: https://github.com/radix-ui/primitives/issues/1077
function onPointerDownOutside(event: Event): void {
    event.preventDefault()
}

/**
 * Renders a Tooltip that will be positioned relative to the wrapped child element. Please reference the examples in Storybook
 * for more details on specific use cases.
 *
 * To support accessibility, our tooltips should:
 * - Be supplemental to the user journey, not essential.
 * - Use clear and concise text.
 * - Not include interactive content (you probably want a `<Popover>` instead).
 *
 * Related accessibility documentation: https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Roles/tooltip_role
 */
export const Tooltip: React.FunctionComponent<TooltipProps> = ({
    children,
    content,
    className,
    defaultOpen = false,
    placement = 'right',
}) => (
    // NOTE: We plan to consolidate this logic with our Popover component in the future, but chose Radix first to support short-term accessibility needs.
    // GitHub issue: https://github.com/sourcegraph/sourcegraph/issues/36080
    <TooltipPrimitive.Root delayDuration={0} defaultOpen={defaultOpen}>
        <TooltipPrimitive.Trigger asChild={true}>
            {/** The onClick and role attributes here are part of the onPointerDownOutside fix described above. */}
            <span
                role="presentation"
                className={classNames(styles.tooltip, className)}
                onClick={event => event.preventDefault()}
            >
                {children}

                {
                    // The rest of the Tooltip components still need to be rendered for the content to correctly be shown conditionally.
                    content === null ? null : (
                        /*
                         * Rendering the Content within the Trigger is a workaround to support being able to hover over the Tooltip content itself.
                         * Refrence: https://github.com/radix-ui/primitives/issues/620#issuecomment-1079147761
                         */
                        <TooltipPrimitive.TooltipContent
                            onPointerDownOutside={onPointerDownOutside}
                            className={styles.tooltipContent}
                            side={placement}
                            role="tooltip"
                        >
                            {content}

                            <TooltipPrimitive.Arrow
                                className={styles.tooltipArrow}
                                height={TOOLTIP_ARROW_HEIGHT}
                                width={TOOLTIP_ARROW_WIDTH}
                            />
                        </TooltipPrimitive.TooltipContent>
                    )
                }
            </span>
        </TooltipPrimitive.Trigger>
    </TooltipPrimitive.Root>
)
