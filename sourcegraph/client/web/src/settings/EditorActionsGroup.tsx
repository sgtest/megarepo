import React from 'react'

import classNames from 'classnames'
import type * as jsonc from 'jsonc-parser'

import { Button, Text } from '@sourcegraph/wildcard'

import styles from './EditorActionsGroup.module.scss'

/**
 * A helper function that modifies site configuration to configure specific
 * common things, such as syncing GitHub repositories.
 */
export type ConfigInsertionFunction = (configJSON: string) => {
    /** The edits to make to the input configuration to insert the new configuration. */
    edits: jsonc.Edit[]

    /** Select text in inserted JSON. */
    selectText?: string | number

    /**
     * If set, the selection is an empty selection that begins at the left-hand match of selectText plus this
     * offset. For example, if selectText is "foo" and cursorOffset is 2, then the final selection will be a cursor
     * "|" positioned as "fo|o".
     */
    cursorOffset?: number
}

export interface EditorAction {
    id: string
    label: string
    run: ConfigInsertionFunction
}

export interface EditorActionsGroupProps {
    actions: EditorAction[]
    onClick: (id: string) => void
}

export const EditorActionsGroup: React.FunctionComponent<EditorActionsGroupProps> = ({ actions, onClick }) => (
    <>
        {actions.length > 0 && (
            <Text className="mb-1">
                <strong>Quick actions:</strong>
            </Text>
        )}
        <div className={classNames(styles.actions, 'mb-2')}>
            {actions.map(({ id, label }) => (
                <Button key={id} onClick={() => onClick(id)} variant="secondary" outline={true} size="sm">
                    {label}
                </Button>
            ))}
        </div>
    </>
)
