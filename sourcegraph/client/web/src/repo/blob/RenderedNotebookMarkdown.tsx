import { noop } from 'lodash'
import React, { useMemo } from 'react'
import * as uuid from 'uuid'

import { NotebookComponent, NotebookComponentProps } from '../../notebooks/notebook/NotebookComponent'
import { convertMarkdownToBlocks } from '../../notebooks/serialize/convertMarkdownToBlocks'

import styles from './RenderedNotebookMarkdown.module.scss'

export const SEARCH_NOTEBOOK_FILE_EXTENSION = '.snb.md'

interface RenderedNotebookMarkdownProps extends Omit<NotebookComponentProps, 'onSerializeBlocks' | 'blocks'> {
    markdown: string
}

export const RenderedNotebookMarkdown: React.FunctionComponent<RenderedNotebookMarkdownProps> = ({
    markdown,
    ...props
}) => {
    // Generate fresh block IDs, since we do not store them in Markdown.
    const blocks = useMemo(() => convertMarkdownToBlocks(markdown).map(block => ({ id: uuid.v4(), ...block })), [
        markdown,
    ])
    return (
        <div className={styles.renderedSearchNotebookMarkdownWrapper}>
            <div className={styles.renderedSearchNotebookMarkdown}>
                <NotebookComponent isReadOnly={true} blocks={blocks} {...props} onSerializeBlocks={noop} />
            </div>
        </div>
    )
}
