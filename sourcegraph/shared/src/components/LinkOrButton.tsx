import * as H from 'history'
import * as React from 'react'
import { Key } from 'ts-key-enum'
import { RouterLinkOrAnchor } from './RouterLinkOrAnchor'

interface Props {
    /** The link destination URL. */
    to?: H.LocationDescriptor

    /** The link target. */
    target?: '_self' | '_blank' | string

    /**
     * Called when the user clicks or presses enter on this element.
     */
    onSelect?: (event: React.MouseEvent | React.KeyboardEvent) => void

    /** A tooltip to display when the user hovers or focuses this element. */
    ['data-tooltip']?: string

    /** The component's CSS class name (defaults to "nav-link"). */
    className?: string

    disabled?: boolean
}

/**
 * A component that is displayed in the same way, regardless of whether it's a link (with a
 * destination URL) or a button (with a click handler).
 *
 * It is keyboard accessible: unlike <Link> or <a>, pressing the enter key triggers it. Unlike
 * <button>, it shows a focus ring.
 */
export class LinkOrButton extends React.PureComponent<Props> {
    public render(): JSX.Element | null {
        const className = `${this.props.className === undefined ? 'nav-link' : this.props.className} ${
            this.props.disabled ? 'disabled' : ''
        }`

        const commonProps = {
            className,
            'data-tooltip': this.props['data-tooltip'],
            'aria-label': this.props['data-tooltip'],
            tabIndex: 0,
            onClick: this.onAnchorClick,
            onKeyPress: this.onAnchorKeyPress,
        }

        if (!this.props.to) {
            // Render using an <a> with no href, so that we get a focus ring (when using Bootstrap).
            // We need to set up a keypress listener because <a onclick> doesn't get triggered by
            // enter.
            return <a {...commonProps}>{this.props.children}</a>
        }

        return (
            <RouterLinkOrAnchor {...commonProps} to={this.props.to} target={this.props.target}>
                {this.props.children}
            </RouterLinkOrAnchor>
        )
    }

    private onAnchorClick: React.MouseEventHandler<HTMLAnchorElement> = e => {
        if (this.props.onSelect) {
            this.props.onSelect(e)
        }
    }

    private onAnchorKeyPress: React.KeyboardEventHandler<HTMLAnchorElement> = e => {
        if (isSelectKeyPress(e)) {
            if (this.props.onSelect) {
                this.props.onSelect(e)
            }
        }
    }
}

function isSelectKeyPress(e: React.KeyboardEvent): boolean {
    return e.key === Key.Enter && !e.ctrlKey && !e.shiftKey && !e.metaKey && !e.altKey
}
