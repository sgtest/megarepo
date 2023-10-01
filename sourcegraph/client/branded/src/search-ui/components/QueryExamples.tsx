import React, { useCallback } from 'react'

import { mdiOpenInNew } from '@mdi/js'
import classNames from 'classnames'

import { SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'
import type { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { buildSearchURLQuery } from '@sourcegraph/shared/src/util/url'
import { H2, Link, Icon, Tabs, TabList, TabPanels, TabPanel, Tab, ButtonLink } from '@sourcegraph/wildcard'

import { exampleQueryColumns } from './QueryExamples.constants'
import { SyntaxHighlightedSearchQuery } from './SyntaxHighlightedSearchQuery'
import { useQueryExamples, type QueryExamplesSection } from './useQueryExamples'

import styles from './QueryExamples.module.scss'

export interface QueryExamplesProps extends TelemetryProps {
    selectedSearchContextSpec?: string
    isSourcegraphDotCom?: boolean
}

export const QueryExamples: React.FunctionComponent<QueryExamplesProps> = ({
    selectedSearchContextSpec,
    telemetryService,
    isSourcegraphDotCom = false,
}) => {
    const exampleSyntaxColumns = useQueryExamples(selectedSearchContextSpec ?? 'global', isSourcegraphDotCom)

    const onQueryExampleClick = useCallback(
        (query: string) => {
            telemetryService.log('QueryExampleClicked', { queryExample: query }, { queryExample: query })
        },
        [telemetryService]
    )

    return isSourcegraphDotCom ? (
        <Tabs size="medium">
            <TabList wrapperClassName={classNames('mb-4', styles.tabHeader)}>
                <Tab>How to search</Tab>
                <Tab>Popular queries</Tab>
            </TabList>
            <TabPanels>
                <TabPanel className={styles.tabPanel}>
                    <QueryExamplesLayout
                        queryColumns={exampleSyntaxColumns}
                        onQueryExampleClick={onQueryExampleClick}
                    />
                </TabPanel>
                <TabPanel className={styles.tabPanel}>
                    <QueryExamplesLayout queryColumns={exampleQueryColumns} onQueryExampleClick={onQueryExampleClick} />
                </TabPanel>
            </TabPanels>
        </Tabs>
    ) : (
        <QueryExamplesLayout queryColumns={exampleSyntaxColumns} onQueryExampleClick={onQueryExampleClick} />
    )
}

interface QueryExamplesLayout {
    queryColumns: QueryExamplesSection[][]
    onQueryExampleClick: (query: string) => void
}

const QueryExamplesLayout: React.FunctionComponent<QueryExamplesLayout> = ({ queryColumns, onQueryExampleClick }) => (
    <div className={styles.queryExamplesSectionsColumns}>
        {queryColumns.map((column, index) => (
            <div key={`column-${queryColumns[index][0].title}`}>
                {column.map(({ title, queryExamples }) => (
                    <ExamplesSection
                        key={title}
                        title={title}
                        queryExamples={queryExamples}
                        onQueryExampleClick={onQueryExampleClick}
                    />
                ))}
                {/* Add docs link to last column */}
                {queryColumns.length === index + 1 && (
                    <small className="d-block">
                        <Link target="blank" to="/help/code_search/reference/queries">
                            Complete query reference{' '}
                            <Icon role="img" aria-label="Open in a new tab" svgPath={mdiOpenInNew} />
                        </Link>
                    </small>
                )}
            </div>
        ))}
    </div>
)

interface ExamplesSection extends QueryExamplesSection {
    onQueryExampleClick: (query: string) => void
}

const ExamplesSection: React.FunctionComponent<ExamplesSection> = ({ title, queryExamples, onQueryExampleClick }) => (
    <div className={styles.queryExamplesSection}>
        <H2 className={styles.queryExamplesSectionTitle}>{title}</H2>
        <ul className={classNames('list-unstyled', styles.queryExamplesItems)}>
            {queryExamples
                .filter(({ query }) => query.length > 0)
                .map(({ query, helperText }) => (
                    <QueryExampleChip key={query} query={query} helperText={helperText} onClick={onQueryExampleClick} />
                ))}
        </ul>
    </div>
)

interface QueryExample {
    query: string
    helperText?: string
}

interface QueryExampleChipProps extends QueryExample {
    className?: string
    onClick: (query: string) => void | undefined
}

const QueryExampleChip: React.FunctionComponent<QueryExampleChipProps> = ({
    query,
    helperText,
    className,
    onClick,
}) => (
    <li className={classNames('d-flex align-items-center', className)}>
        <ButtonLink
            className={styles.queryExampleChip}
            to={`/search?${buildSearchURLQuery(query, SearchPatternType.standard, false)}`}
            onClick={() => onClick(query)}
        >
            <SyntaxHighlightedSearchQuery query={query} searchPatternType={SearchPatternType.standard} />
        </ButtonLink>
        {helperText && (
            <span className="text-muted ml-2">
                <small>{helperText}</small>
            </span>
        )}
    </li>
)
