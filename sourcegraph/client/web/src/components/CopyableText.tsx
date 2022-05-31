import * as React from 'react'

import classNames from 'classnames'
import copy from 'copy-to-clipboard'
import ContentCopyIcon from 'mdi-react/ContentCopyIcon'

import { Button, Icon, Input } from '@sourcegraph/wildcard'

import styles from './CopyableText.module.scss'

interface Props {
    /** The text to present and to copy. */
    text: string

    /** An optional class name. */
    className?: string

    /** Whether or not the input should take up all horizontal space (flex:1) */
    flex?: boolean

    /** The size of the input element. */
    size?: number

    /** Whether or not the text to be copied is a password. */
    password?: boolean

    /** The label used for screen readers */
    label?: string

    /** Callback for when the content is copied  */
    onCopy?: () => void
}

interface State {
    /** Whether the text was just copied. */
    copied: boolean
}

/**
 * A component that displays a single line of text and a copy-to-clipboard button. There are other
 * niceties, such as triple-clicking selects only the text and not other adjacent components' text
 * labels.
 */
export class CopyableText extends React.PureComponent<Props, State> {
    public state: State = { copied: false }

    public render(): JSX.Element | null {
        return (
            <div className={classNames('form-inline', this.props.className)}>
                <div className={classNames('input-group', this.props.flex && 'flex-1')}>
                    <Input
                        type={this.props.password ? 'password' : 'text'}
                        inputClassName={styles.input}
                        aria-label={this.props.label}
                        value={this.props.text}
                        size={this.props.size}
                        readOnly={true}
                        onClick={this.onClickInput}
                    />
                    <div className="input-group-append">
                        <Button
                            onClick={this.onClickButton}
                            disabled={this.state.copied}
                            variant="secondary"
                            aria-label="Copy"
                        >
                            <Icon role="img" as={ContentCopyIcon} aria-hidden={true} />{' '}
                            {this.state.copied ? 'Copied' : 'Copy'}
                        </Button>
                    </div>
                </div>
            </div>
        )
    }

    private onClickInput: React.MouseEventHandler<HTMLInputElement> = event => {
        event.currentTarget.focus()
        event.currentTarget.setSelectionRange(0, this.props.text.length)
        this.copyToClipboard()
    }

    private onClickButton = (): void => this.copyToClipboard()

    private copyToClipboard(): void {
        copy(this.props.text)
        this.setState({ copied: true })

        setTimeout(() => this.setState({ copied: false }), 1000)

        if (typeof this.props.onCopy === 'function') {
            this.props.onCopy()
        }
    }
}
