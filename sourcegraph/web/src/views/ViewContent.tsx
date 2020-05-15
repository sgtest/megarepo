import React from 'react'
import { Markdown } from '../../../shared/src/components/Markdown'
import { renderMarkdown } from '../../../shared/src/util/markdown'
import { MarkupKind } from '@sourcegraph/extension-api-classes'
import H from 'history'
import { QueryInputInViewContent } from './QueryInputInViewContent'
import { View, MarkupContent } from 'sourcegraph'
import { CaseSensitivityProps, PatternTypeProps, CopyQueryButtonProps } from '../search'
import { SettingsCascadeProps } from '../../../shared/src/settings/settings'
import { hasProperty } from '../../../shared/src/util/types'
import { isObject } from 'lodash'
import { VersionContextProps } from '../../../shared/src/search/util'
import { ChartViewContent } from './ChartViewContent'

const isMarkupContent = (input: unknown): input is MarkupContent =>
    isObject(input) && hasProperty('value')(input) && typeof input.value === 'string'

export interface ViewContentProps
    extends SettingsCascadeProps,
        PatternTypeProps,
        CaseSensitivityProps,
        CopyQueryButtonProps,
        VersionContextProps {
    viewContent: View['content']
    location: H.Location
    history: H.History
}

/**
 * Renders the content of an extension-contributed view.
 */
export const ViewContent: React.FunctionComponent<ViewContentProps> = ({ viewContent, ...props }) => (
    <div className="view-content">
        {viewContent.map((content, i) =>
            isMarkupContent(content) ? (
                <React.Fragment key={i}>
                    {content.kind === MarkupKind.Markdown || !content.kind ? (
                        <Markdown
                            className="view-content__markdown mb-1"
                            dangerousInnerHTML={renderMarkdown(content.value)}
                            history={props.history}
                        />
                    ) : (
                        content.value
                    )}
                </React.Fragment>
            ) : 'chart' in content ? (
                <ChartViewContent key={i} content={content} history={props.history} />
            ) : content.component === 'QueryInput' ? (
                <QueryInputInViewContent
                    {...props}
                    key={i}
                    implicitQueryPrefix={
                        typeof content.props.implicitQueryPrefix === 'string' ? content.props.implicitQueryPrefix : ''
                    }
                />
            ) : null
        )}
    </div>
)
