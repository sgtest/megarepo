import * as React from 'react'
import { OpenInSourcegraphProps } from '../repo'
import { getPlatformName } from '../util/context'
import { SourcegraphIconButton, SourcegraphIconButtonProps } from './Button'
import classNames from 'classnames'

interface Props extends SourcegraphIconButtonProps {
    openProps: OpenInSourcegraphProps
}

export class OpenOnSourcegraph extends React.Component<Props, {}> {
    public render(): JSX.Element {
        const url = this.getOpenInSourcegraphUrl(this.props.openProps)
        return (
            <SourcegraphIconButton
                {...this.props}
                className={classNames('open-on-sourcegraph', this.props.className)}
                href={url}
            />
        )
    }

    private getOpenInSourcegraphUrl(props: OpenInSourcegraphProps): string {
        const baseUrl = props.sourcegraphURL
        // Build URL for Web
        let url = `${baseUrl}/${props.repoName}`
        if (props.commit) {
            return `${url}/-/compare/${props.commit.baseRev}...${props.commit.headRev}?utm_source=${getPlatformName()}`
        }
        if (props.rev) {
            url = `${url}@${props.rev}`
        }
        if (props.filePath) {
            url = `${url}/-/blob/${props.filePath}`
        }
        if (props.query) {
            if (props.query.diff) {
                url = `${url}?diff=${props.query.diff.rev}&utm_source=${getPlatformName()}`
            } else if (props.query.search) {
                url = `${url}?q=${props.query.search}&utm_source=${getPlatformName()}`
            }
        }
        if (props.coords) {
            url = `${url}#L${props.coords.line}:${props.coords.char}`
        }
        if (props.fragment) {
            url = `${url}$${props.fragment}`
        }
        return url
    }
}
