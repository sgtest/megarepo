import copy from 'copy-to-clipboard'
import ContentCopyIcon from 'mdi-react/ContentCopyIcon'
import * as React from 'react'

interface Props {
    /** The text to present and to copy. */
    text: string

    /** An optional class name. */
    className?: string

    /** The size of the input element. */
    size?: number
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
            <div className={`copyable-text form-inline ${this.props.className || ''}`}>
                <div className="input-group">
                    <input
                        type="text"
                        className="copyable-text__input form-control"
                        value={this.props.text}
                        size={this.props.size}
                        readOnly={true}
                        onClick={this.onClickInput}
                    />
                    <div className="input-group-append">
                        <button className="btn btn-secondary" onClick={this.onClickButton} disabled={this.state.copied}>
                            <ContentCopyIcon className="icon-inline" /> {this.state.copied ? 'Copied' : 'Copy'}
                        </button>
                    </div>
                </div>
            </div>
        )
    }

    private onClickInput: React.MouseEventHandler<HTMLInputElement> = e => {
        e.currentTarget.focus()
        e.currentTarget.setSelectionRange(0, this.props.text.length)
        this.copyToClipboard()
    }

    private onClickButton = () => this.copyToClipboard()

    private copyToClipboard(): void {
        copy(this.props.text)
        this.setState({ copied: true })

        setTimeout(() => this.setState({ copied: false }), 1000)
    }
}
