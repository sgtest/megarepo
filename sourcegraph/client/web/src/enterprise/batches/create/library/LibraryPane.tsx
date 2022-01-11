import ChevronDoubleLeftIcon from 'mdi-react/ChevronDoubleLeftIcon'
import ChevronDoubleRightIcon from 'mdi-react/ChevronDoubleRightIcon'
import React, { useState, useCallback } from 'react'
import { Collapse } from 'reactstrap'

import { Button } from '@sourcegraph/wildcard'

import { Scalars } from '../../../../graphql-operations'
import { insertNameIntoLibraryItem } from '../yaml-util'

import combySample from './comby.batch.yaml'
import goImportsSample from './go-imports.batch.yaml'
import helloWorldSample from './hello-world.batch.yaml'
import styles from './LibraryPane.module.scss'
import minimalSample from './minimal.batch.yaml'
import { ReplaceSpecModal } from './ReplaceSpecModal'

interface LibraryItem {
    name: string
    code: string
}

const LIBRARY: [LibraryItem, LibraryItem, LibraryItem, LibraryItem] = [
    { name: 'hello world', code: helloWorldSample },
    { name: 'minimal', code: minimalSample },
    { name: 'modify with comby', code: combySample },
    { name: 'update go imports', code: goImportsSample },
]

interface LibraryPaneProps {
    /**
     * The name of the batch change, used for automatically filling in the name for any
     * item selected from the library.
     */
    name: Scalars['String']
    onReplaceItem: (item: string) => void
}

export const LibraryPane: React.FunctionComponent<LibraryPaneProps> = ({ name, onReplaceItem }) => {
    const [collapsed, setCollapsed] = useState(false)
    const [selectedItem, setSelectedItem] = useState<LibraryItem>()

    const onConfirm = useCallback(() => {
        if (selectedItem) {
            const codeWithName = insertNameIntoLibraryItem(selectedItem.code, name)
            onReplaceItem(codeWithName)
            setSelectedItem(undefined)
        }
    }, [name, selectedItem, onReplaceItem])

    return (
        <>
            {selectedItem ? (
                <ReplaceSpecModal
                    libraryItemName={selectedItem.name}
                    onCancel={() => setSelectedItem(undefined)}
                    onConfirm={onConfirm}
                />
            ) : null}
            <div className="d-flex flex-column">
                <div className="d-flex align-items-center justify-space-between flex-0">
                    <h5 className="flex-grow-1">Library</h5>
                    <Button
                        className="flex-0"
                        onClick={() => setCollapsed(!collapsed)}
                        aria-label={collapsed ? 'Expand' : 'Collapse'}
                    >
                        {collapsed ? (
                            <ChevronDoubleRightIcon className="icon-inline mr-1" />
                        ) : (
                            <ChevronDoubleLeftIcon className="icon-inline mr-1" />
                        )}
                    </Button>
                </div>

                {/* TODO: This should slide vertically but not on our version of reactstrap. */}
                <Collapse className={styles.collapseContainer} isOpen={!collapsed}>
                    <ul className="m-0 p-0">
                        {LIBRARY.map(item => (
                            <li className={styles.libraryItem} key={item.name}>
                                <button
                                    type="button"
                                    className={styles.libraryItemButton}
                                    onClick={() => setSelectedItem(item)}
                                >
                                    {item.name}
                                </button>
                            </li>
                        ))}
                    </ul>
                </Collapse>
            </div>
        </>
    )
}
