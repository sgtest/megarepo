import React, { FC, useCallback, useRef, useState } from 'react'

import { mdiFileDocumentOutline, mdiFolderOutline, mdiMenuDown, mdiMenuUp } from '@mdi/js'
import classNames from 'classnames'
import * as H from 'history'

import { TreeFields } from '@sourcegraph/shared/src/graphql-operations'
import {
    Card,
    CardHeader,
    Icon,
    Link,
    LinkOrSpan,
    ParentSize,
    StackedMeter,
    Tooltip,
    useElementObscuredArea,
} from '@sourcegraph/wildcard'

import { RenderedFile } from '../blob/RenderedFile'

import styles from './TreePagePanels.module.scss'

interface ReadmePreviewCardProps {
    readmeHTML: string
    readmeURL: string
    location: H.Location
    className?: string
}

export const ReadmePreviewCard: React.FunctionComponent<ReadmePreviewCardProps> = props => {
    const { readmeHTML, readmeURL, location, className } = props

    const renderedFileRef = useRef<HTMLDivElement>(null)
    const { bottom } = useElementObscuredArea(renderedFileRef)

    return (
        <section className={className}>
            <RenderedFile
                key={readmeHTML}
                ref={renderedFileRef}
                location={location}
                dangerousInnerHTML={readmeHTML}
                className={styles.readme}
            />
            {bottom > 0 && (
                <>
                    <div className={styles.readmeFader} />
                    <Link to={readmeURL} className={styles.readmeMoreLink}>
                        View full README
                    </Link>
                </>
            )}
        </section>
    )
}

export interface DiffStat {
    path: string
    added: number
    deleted: number
}

export interface FilePanelProps {
    entries: Pick<TreeFields['entries'][number], 'name' | 'url' | 'isDirectory' | 'path'>[]
    diffStats?: DiffStat[]
    className?: string
}

export const FilesCard: FC<FilePanelProps> = props => {
    const { entries, diffStats, className } = props

    const [sortColumn, setSortColumn] = useState<{
        column: 'Files' | 'Activity'
        direction: 'asc' | 'desc'
    }>({ column: 'Files', direction: 'asc' })

    const diffStatsByPath: { [path: string]: DiffStat } = {}
    let maxLinesChanged = 1
    if (diffStats) {
        for (const diffStat of diffStats) {
            if (diffStat.added + diffStat.deleted > maxLinesChanged) {
                maxLinesChanged = diffStat.added + diffStat.deleted
            }
            diffStatsByPath[diffStat.path] = diffStat
        }
    }

    let sortedEntries = [...entries]
    const { column, direction } = sortColumn
    switch (column) {
        case 'Files':
            if (direction === 'desc') {
                sortedEntries.reverse()
            }
            break
        case 'Activity':
            sortedEntries = [...entries]
            if (diffStats) {
                sortedEntries.sort((entry1, entry2) => {
                    const stats1: DiffStat = diffStatsByPath[entry1.name]
                    const stats2: DiffStat = diffStatsByPath[entry2.name]
                    let difference =
                        (stats2 ? stats2.added + stats2.deleted : 0) - (stats1 ? stats1.added + stats1.deleted : 0)
                    if (direction === 'desc') {
                        difference *= -1
                    }
                    return difference
                })
            }
            break
    }

    const sortCallback = useCallback(
        (column: 'Files' | 'Activity'): void => {
            if (sortColumn.column === column && sortColumn.direction === 'asc') {
                setSortColumn({ column, direction: 'desc' })
            } else {
                setSortColumn({ column, direction: 'asc' })
            }
        },
        [sortColumn]
    )
    const clickFiles = useCallback(() => sortCallback('Files'), [sortCallback])
    const keydownFiles = useCallback(
        ({ key }: React.KeyboardEvent<HTMLDivElement>) => key === 'Enter' && sortCallback('Files'),
        [sortCallback]
    )
    const clickActivity = useCallback(() => sortCallback('Activity'), [sortCallback])
    const keydownActivity = useCallback(
        ({ key }: React.KeyboardEvent<HTMLDivElement>) => key === 'Enter' && sortCallback('Activity'),
        [sortCallback]
    )

    interface Datum {
        name: 'deleted' | 'added'
        value: number
        className: string
    }

    const getDatumValue = useCallback((datum: Datum) => datum.value, [])
    const getDatumName = useCallback((datum: Datum) => datum.name, [])
    const getDatumClassName = useCallback((datum: Datum) => datum.className, [])

    return (
        <Card className={className}>
            <CardHeader className={styles.cardColHeaderWrapper}>
                <div className="container-fluid px-2">
                    <div className="row">
                        <div
                            role="button"
                            tabIndex={0}
                            onClick={clickFiles}
                            onKeyDown={keydownFiles}
                            className={classNames('d-flex flex-row align-items-start col-9 px-2', styles.cardColHeader)}
                        >
                            Files
                            <div className="flex-shrink-1 d-flex flex-column">
                                <Icon
                                    aria-label="Sort ascending"
                                    svgPath={mdiMenuUp}
                                    className={classNames(
                                        styles.sortDscIcon,
                                        sortColumn.column === 'Files' &&
                                            sortColumn.direction === 'desc' &&
                                            styles.sortSelectedIcon
                                    )}
                                />
                                <Icon
                                    aria-label="Sort descending"
                                    svgPath={mdiMenuDown}
                                    className={classNames(
                                        styles.sortAscIcon,
                                        sortColumn.column === 'Files' &&
                                            sortColumn.direction === 'asc' &&
                                            styles.sortSelectedIcon
                                    )}
                                />
                            </div>
                        </div>
                        <div
                            title="1 month activity"
                            role="button"
                            tabIndex={0}
                            onClick={clickActivity}
                            onKeyDown={keydownActivity}
                            className={classNames(
                                'd-flex flex-row-reverse align-items-start col-3 px-2 text-right',
                                styles.cardColHeader
                            )}
                        >
                            <div className="flex-shrink-1 d-flex flex-column">
                                <Icon
                                    aria-label="Sort ascending"
                                    svgPath={mdiMenuUp}
                                    className={classNames(
                                        styles.sortDscIcon,
                                        sortColumn.column === 'Activity' &&
                                            sortColumn.direction === 'desc' &&
                                            styles.sortSelectedIcon
                                    )}
                                />
                                <Icon
                                    aria-label="Sort descending"
                                    svgPath={mdiMenuDown}
                                    className={classNames(
                                        styles.sortAscIcon,
                                        sortColumn.column === 'Activity' &&
                                            sortColumn.direction === 'asc' &&
                                            styles.sortSelectedIcon
                                    )}
                                />
                            </div>
                            Recent activity
                        </div>
                    </div>
                </div>
            </CardHeader>
            <div className="container-fluid">
                {sortedEntries.map(entry => (
                    <div key={entry.name} className={classNames('row', styles.fileItem)}>
                        <div className="list-group list-group-flush px-2 py-1 col-9">
                            <LinkOrSpan
                                to={entry.url}
                                className={classNames(
                                    'test-page-file-decorable',
                                    styles.treeEntry,
                                    entry.isDirectory && 'font-weight-bold',
                                    `test-tree-entry-${entry.isDirectory ? 'directory' : 'file'}`
                                )}
                                title={entry.path}
                                data-testid="tree-entry"
                            >
                                <div
                                    className={classNames(
                                        'd-flex align-items-center justify-content-between test-file-decorable-name overflow-hidden'
                                    )}
                                >
                                    <span>
                                        <Icon
                                            className="mr-1"
                                            svgPath={entry.isDirectory ? mdiFolderOutline : mdiFileDocumentOutline}
                                            aria-hidden={true}
                                        />
                                        {entry.name}
                                        {entry.isDirectory && '/'}
                                    </span>
                                </div>
                            </LinkOrSpan>
                        </div>
                        <div className="list-group list-group-flush px-2 py-1 col-3">
                            {diffStatsByPath[entry.name] && (
                                <Tooltip
                                    placement="topEnd"
                                    content={`${Intl.NumberFormat('en', {
                                        notation: 'compact',
                                    }).format(diffStatsByPath[entry.name].added)} lines added,\n${Intl.NumberFormat(
                                        'en',
                                        {
                                            notation: 'compact',
                                        }
                                    ).format(diffStatsByPath[entry.name].deleted)} lines removed in the past 30 days`}
                                >
                                    <div className={styles.meterContainer}>
                                        <ParentSize>
                                            {({ width }) => (
                                                <StackedMeter
                                                    width={width}
                                                    height={10}
                                                    viewMinMax={[0, maxLinesChanged]}
                                                    data={[
                                                        {
                                                            name: 'deleted',
                                                            value: diffStatsByPath[entry.name].deleted,
                                                            className: styles.diffStatDeleted,
                                                        },
                                                        {
                                                            name: 'added',
                                                            value: diffStatsByPath[entry.name].added,
                                                            className: styles.diffStatAdded,
                                                        },
                                                    ]}
                                                    getDatumValue={getDatumValue}
                                                    getDatumName={getDatumName}
                                                    getDatumClassName={getDatumClassName}
                                                    minBarWidth={10}
                                                    className={styles.barSvg}
                                                    rightToLeft={true}
                                                />
                                            )}
                                        </ParentSize>
                                    </div>
                                </Tooltip>
                            )}
                        </div>
                    </div>
                ))}
            </div>
        </Card>
    )
}
