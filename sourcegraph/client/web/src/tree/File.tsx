/* eslint jsx-a11y/click-events-have-key-events: warn, jsx-a11y/no-static-element-interactions: warn */
import * as React from 'react'
import { useEffect, useState } from 'react'

import { mdiSourceRepository } from '@mdi/js'
import classNames from 'classnames'
import * as H from 'history'
import { escapeRegExp, isEqual } from 'lodash'
import { NavLink } from 'react-router-dom'
import { FileDecoration } from 'sourcegraph'

import { gql, useQuery } from '@sourcegraph/http-client'
import { useCoreWorkflowImprovementsEnabled } from '@sourcegraph/shared/src/settings/useCoreWorkflowImprovementsEnabled'
import { SymbolTag } from '@sourcegraph/shared/src/symbols/SymbolTag'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { Icon, LoadingSpinner } from '@sourcegraph/wildcard'

import { InlineSymbolsResult, Scalars } from '../graphql-operations'
import { parseBrowserRepoURL } from '../util/url'

import {
    TreeLayerCell,
    TreeLayerRowContents,
    TreeLayerRowContentsLink,
    TreeRowAlert,
    TreeLayerRowContentsText,
    TreeRowIcon,
    TreeRowLabel,
    TreeRow,
} from './components'
import { FileDecorator } from './FileDecorator'
import { TreeLayerProps } from './TreeLayer'
import { maxEntries, TreeEntryInfo, treePadding } from './util'

import styles from './File.module.scss'

interface FileProps extends ThemeProps {
    fileDecorations?: FileDecoration[]
    entryInfo: TreeEntryInfo
    depth: number
    index: number
    className?: string
    maxEntries: number
    handleTreeClick: () => void
    noopRowClick: (event: React.MouseEvent<HTMLAnchorElement>) => void
    linkRowClick: (event: React.MouseEvent<HTMLAnchorElement>) => void
    isActive: boolean
    isSelected: boolean

    // For core workflow inline symbols redesign
    repoID: Scalars['ID']
    revision: string
    location: H.Location
}

export const File: React.FunctionComponent<React.PropsWithChildren<FileProps>> = props => {
    const [coreWorkflowImprovementsEnabled] = useCoreWorkflowImprovementsEnabled()

    const renderedFileDecorations = (
        <FileDecorator
            // If component is not specified, or it is 'sidebar', render it.
            fileDecorations={props.fileDecorations?.filter(decoration => decoration?.where !== 'page')}
            isLightTheme={props.isLightTheme}
            isActive={props.isActive}
        />
    )

    const padding = treePadding(props.depth, false)

    return (
        <>
            <TreeRow
                key={props.entryInfo.path}
                className={props.className}
                isActive={props.isActive}
                isSelected={props.isSelected}
            >
                <TreeLayerCell className="test-sidebar-file-decorable">
                    {props.entryInfo.submodule ? (
                        props.entryInfo.url ? (
                            <TreeLayerRowContentsLink
                                to={props.entryInfo.url}
                                onClick={props.linkRowClick}
                                draggable={false}
                                title={'Submodule: ' + props.entryInfo.submodule.url}
                                data-tree-path={props.entryInfo.path}
                            >
                                <TreeLayerRowContentsText>
                                    {/* TODO Improve accessibility: https://github.com/sourcegraph/sourcegraph/issues/12916 */}
                                    <TreeRowIcon style={treePadding(props.depth, true)} onClick={props.noopRowClick}>
                                        <Icon aria-hidden={true} svgPath={mdiSourceRepository} />
                                    </TreeRowIcon>
                                    <TreeRowLabel className="test-file-decorable-name">
                                        {props.entryInfo.name} @ {props.entryInfo.submodule.commit.slice(0, 7)}
                                    </TreeRowLabel>
                                    {renderedFileDecorations}
                                </TreeLayerRowContentsText>
                            </TreeLayerRowContentsLink>
                        ) : (
                            <TreeLayerRowContents title={'Submodule: ' + props.entryInfo.submodule.url}>
                                <TreeLayerRowContentsText>
                                    <TreeRowIcon style={treePadding(props.depth, true)}>
                                        <Icon aria-hidden={true} svgPath={mdiSourceRepository} />
                                    </TreeRowIcon>
                                    <TreeRowLabel className="test-file-decorable-name">
                                        {props.entryInfo.name} @ {props.entryInfo.submodule.commit.slice(0, 7)}
                                    </TreeRowLabel>
                                    {renderedFileDecorations}
                                </TreeLayerRowContentsText>
                            </TreeLayerRowContents>
                        )
                    ) : (
                        <TreeLayerRowContentsLink
                            className="test-tree-file-link"
                            to={props.entryInfo.url}
                            onClick={props.linkRowClick}
                            data-tree-path={props.entryInfo.path}
                            draggable={false}
                            title={props.entryInfo.path}
                            // needed because of dynamic styling
                            style={padding}
                            tabIndex={-1}
                        >
                            <TreeLayerRowContentsText className="d-flex flex-row flex-1 justify-content-between">
                                <span className="test-file-decorable-name">{props.entryInfo.name}</span>
                                {renderedFileDecorations}
                            </TreeLayerRowContentsText>
                        </TreeLayerRowContentsLink>
                    )}
                    {props.index === maxEntries - 1 && (
                        <TreeRowAlert
                            variant="warning"
                            style={treePadding(props.depth, true)}
                            error="Too many entries. Use search to find a specific file."
                        />
                    )}
                </TreeLayerCell>
            </TreeRow>
            {coreWorkflowImprovementsEnabled && props.isActive && (
                <Symbols
                    repoID={props.repoID}
                    revision={props.revision}
                    activePath={props.entryInfo.path}
                    location={props.location}
                    style={padding}
                />
            )}
        </>
    )
}

export const SYMBOLS_QUERY = gql`
    query InlineSymbols($repo: ID!, $revision: String!, $includePatterns: [String!]) {
        node(id: $repo) {
            __typename
            ... on Repository {
                commit(rev: $revision) {
                    symbols(first: 1000, query: "", includePatterns: $includePatterns) {
                        ...InlineSymbolConnectionFields
                    }
                }
            }
        }
    }

    fragment InlineSymbolConnectionFields on SymbolConnection {
        __typename
        nodes {
            ...InlineSymbolNodeFields
        }
    }

    fragment InlineSymbolNodeFields on Symbol {
        __typename
        name
        containerName
        kind
        language
        location {
            resource {
                path
            }
            range {
                start {
                    line
                    character
                }
                end {
                    line
                    character
                }
            }
        }
        url
    }
`

interface SymbolsProps
    extends Pick<TreeLayerProps, 'repoID' | 'revision' | 'activePath' | 'location'>,
        Pick<React.HTMLAttributes<HTMLDivElement>, 'style'> {}

const Symbols: React.FunctionComponent<SymbolsProps> = ({ repoID, revision, activePath, location, style }) => {
    const { data, loading, error } = useQuery<InlineSymbolsResult>(SYMBOLS_QUERY, {
        variables: {
            repo: repoID,
            revision,
            // `includePatterns` expects regexes, so first escape the path.
            includePatterns: ['^' + escapeRegExp(activePath)],
        },
    })

    const currentLocation = parseBrowserRepoURL(H.createPath(location))
    const isSymbolActive = (symbolUrl: string): boolean => {
        const symbolLocation = parseBrowserRepoURL(symbolUrl)
        return (
            currentLocation.repoName === symbolLocation.repoName &&
            currentLocation.revision === symbolLocation.revision &&
            currentLocation.filePath === symbolLocation.filePath &&
            isEqual(currentLocation.position, symbolLocation.position)
        )
    }

    if (loading) {
        return (
            <Delay timeout={800}>
                <TreeRow className={styles.symbols}>
                    <TreeLayerCell>
                        <TreeLayerRowContents className="d-flex" style={style}>
                            <LoadingSpinner /> Loading symbol data...
                        </TreeLayerRowContents>
                    </TreeLayerCell>
                </TreeRow>
            </Delay>
        )
    }

    let content = null

    if (error) {
        content = 'Unable to load symbol data.'
    }

    if (data && data.node?.__typename === 'Repository') {
        // Only consider top-level symbols
        const symbols = data.node.commit?.symbols.nodes.filter(symbol => !symbol.containerName) ?? []
        if (symbols.length > 0) {
            content = (
                <ul>
                    {symbols.map(symbol => (
                        <li key={symbol.url}>
                            <NavLink
                                to={symbol.url}
                                isActive={() => isSymbolActive(symbol.url)}
                                className={classNames('test-symbol-link', styles.link)}
                                activeClassName={styles.linkActive}
                            >
                                <SymbolTag kind={symbol.kind} className="mr-1 test-symbol-icon" />
                                <span className={classNames('test-symbol-name')}>{symbol.name}</span>
                            </NavLink>
                        </li>
                    ))}
                </ul>
            )
        } else {
            content = 'No symbols found.'
        }
    }

    if (content) {
        return (
            <TreeRow className={styles.symbols}>
                <TreeLayerCell>
                    <TreeLayerRowContents style={style}>{content}</TreeLayerRowContents>
                </TreeLayerCell>
            </TreeRow>
        )
    }
    return null
}

const Delay: React.FunctionComponent<{ timeout: number; children: React.ReactElement }> = ({ timeout, children }) => {
    const [show, setShow] = useState(false)

    useEffect(() => {
        const id = setTimeout(() => setShow(true), timeout)
        return () => clearTimeout(id)
    }, [timeout])

    return show ? children : null
}
