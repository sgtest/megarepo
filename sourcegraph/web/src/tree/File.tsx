import * as React from 'react'
import { Link } from 'react-router-dom'
import { RepositoryIcon } from '../util/icons'
import { TreeLayerProps } from './TreeLayer'
import { maxEntries, treePadding } from './util'

interface FileProps extends TreeLayerProps {
    className: string
    maxEntries: number
    handleTreeClick: () => void
    noopRowClick: (e: React.MouseEvent<HTMLAnchorElement>) => void
    linkRowClick: (e: React.MouseEvent<HTMLAnchorElement>) => void
}

export const File: React.SFC<FileProps> = (props: FileProps) => (
    <tr key={props.entryInfo.path} className={props.className}>
        <td className="tree__cell">
            {props.entryInfo.submodule ? (
                props.entryInfo.url ? (
                    <Link
                        to={props.entryInfo.url}
                        onClick={props.linkRowClick}
                        draggable={false}
                        title={'Submodule: ' + props.entryInfo.submodule.url}
                        className="tree__row-contents"
                        data-tree-path={props.entryInfo.path}
                    >
                        <div className="tree__row-contents-text">
                            <span
                                className="tree__row-icon"
                                onClick={props.noopRowClick}
                                // tslint:disable-next-line:jsx-ban-props (needed because of dynamic styling)
                                style={treePadding(props.depth, true)}
                                tabIndex={-1}
                            >
                                <RepositoryIcon className="icon-inline" />
                            </span>
                            <span className="tree__row-label">
                                {props.entryInfo.name} @ {props.entryInfo.submodule.commit.substr(0, 7)}
                            </span>
                        </div>
                    </Link>
                ) : (
                    <div className="tree__row-contents" title={'Submodule: ' + props.entryInfo.submodule.url}>
                        <div className="tree__row-contents-text">
                            <span
                                className="tree__row-icon"
                                // tslint:disable-next-line:jsx-ban-props (needed because of dynamic styling)
                                style={treePadding(props.depth, true)}
                            >
                                <RepositoryIcon className="icon-inline" />
                            </span>
                            <span className="tree__row-label">
                                {props.entryInfo.name} @ {props.entryInfo.submodule.commit.substr(0, 7)}
                            </span>
                        </div>
                    </div>
                )
            ) : (
                <Link
                    className="tree__row-contents"
                    to={props.entryInfo.url}
                    onClick={props.linkRowClick}
                    data-tree-path={props.entryInfo.path}
                    draggable={false}
                    title={props.entryInfo.path}
                    // tslint:disable-next-line:jsx-ban-props (needed because of dynamic styling)
                    style={treePadding(props.depth, false)}
                    tabIndex={-1}
                >
                    {props.entryInfo.name}
                </Link>
            )}
            {props.index === maxEntries - 1 && (
                <div
                    className="tree__row-alert alert alert-warning"
                    // tslint:disable-next-line:jsx-ban-props (needed because of dynamic styling)
                    style={treePadding(props.depth, true)}
                >
                    Too many entries. Use search to find a specific file.
                </div>
            )}
        </td>
    </tr>
)
