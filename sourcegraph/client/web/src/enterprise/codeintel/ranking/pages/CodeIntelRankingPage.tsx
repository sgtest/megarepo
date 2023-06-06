import { FunctionComponent, useCallback, useEffect } from 'react'

import { mdiTrashCan } from '@mdi/js'
import classNames from 'classnames'
import { format, formatDistance, parseISO } from 'date-fns'

import { Timestamp } from '@sourcegraph/branded/src/components/Timestamp'
import { useMutation } from '@sourcegraph/http-client'
import { TelemetryProps, TelemetryService } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Button, Code, Container, ErrorAlert, H4, Icon, LoadingSpinner, PageHeader, Text } from '@sourcegraph/wildcard'

import { Collapsible } from '../../../../components/Collapsible'
import {
    BumpDerivativeGraphKeyResult,
    BumpDerivativeGraphKeyVariables,
    DeleteRankingProgressResult,
    DeleteRankingProgressVariables,
} from '../../../../graphql-operations'

import {
    BUMP_DERIVATIVE_GRAPH_KEY,
    DELETE_RANKING_PROGRESS,
    useRankingSummary as defaultUseRankingSummary,
} from './backend'

import styles from './CodeIntelRankingPage.module.scss'

export interface CodeIntelRankingPageProps extends TelemetryProps {
    useRankingSummary?: typeof defaultUseRankingSummary
    telemetryService: TelemetryService
}

export const CodeIntelRankingPage: FunctionComponent<CodeIntelRankingPageProps> = ({
    useRankingSummary = defaultUseRankingSummary,
    telemetryService,
}) => {
    useEffect(() => telemetryService.logViewEvent('CodeIntelRankingPage'), [telemetryService])

    const { data, loading, error, refetch } = useRankingSummary({})

    const [bumpDerivativeGraphKey, { loading: bumping }] = useMutation<
        BumpDerivativeGraphKeyResult,
        BumpDerivativeGraphKeyVariables
    >(BUMP_DERIVATIVE_GRAPH_KEY)

    const [deleteProgressEntry, { loading: deleting }] = useMutation<
        DeleteRankingProgressResult,
        DeleteRankingProgressVariables
    >(DELETE_RANKING_PROGRESS)

    const onEnqueue = useCallback(async () => {
        try {
            await bumpDerivativeGraphKey()
        } finally {
            window.alert('A new job will begin on the next invocation.')
        }
    }, [bumpDerivativeGraphKey])

    const onDelete = useCallback(
        async (graphKey: string) => {
            if (!window.confirm('Delete progress record?')) {
                return
            }

            try {
                await deleteProgressEntry({ variables: { graphKey } })
            } finally {
                await refetch()
            }
        },
        [deleteProgressEntry, refetch]
    )

    if (loading) {
        return <LoadingSpinner />
    }

    if (error) {
        return <ErrorAlert prefix="Failed to load code intelligence summary for repository" error={error} />
    }

    return (
        <>
            <PageHeader
                headingElement="h2"
                path={[
                    {
                        text: <>Ranking calculation history</>,
                    },
                ]}
                description="View the history of ranking calculation."
                className="mb-3"
                actions={
                    <Button onClick={() => onEnqueue()} disabled={bumping || deleting} variant="secondary">
                        Start new ranking map/reduce job
                    </Button>
                }
            />

            {data?.rankingSummary && (
                <>
                    {data.rankingSummary.nextJobStartsAt && (
                        <Text size="small" className="text-right">
                            Next job will begin <Timestamp date={data.rankingSummary.nextJobStartsAt} />.
                        </Text>
                    )}

                    {data.rankingSummary.rankingSummary.length === 0 ? (
                        <Container>
                            <>No data.</>
                        </Container>
                    ) : (
                        data.rankingSummary.rankingSummary.map((summary, index) => (
                            <Summary
                                key={summary.graphKey}
                                summary={summary}
                                onDelete={index > 0 ? () => onDelete(summary.graphKey) : undefined}
                                expanded={index === 0}
                                className="mb-3"
                            />
                        ))
                    )}
                </>
            )}
        </>
    )
}

interface Summary {
    graphKey: string
    pathMapperProgress: Progress
    referenceMapperProgress: Progress
    reducerProgress: Progress | null
}

interface Progress {
    startedAt: string
    completedAt: string | null
    processed: number
    total: number
}

interface SummaryProps {
    summary: Summary
    onDelete?: () => Promise<void>
    expanded?: boolean
    className?: string
}

const Summary: FunctionComponent<SummaryProps> = ({ summary, onDelete, expanded = true, className = '' }) => (
    <Container className={className}>
        <Collapsible
            title={
                <>
                    <Code>{summary.graphKey}</Code>
                </>
            }
            titleAtStart={true}
            defaultExpanded={expanded}
        >
            <div className="pt-4">
                <Progress
                    title="Path mapper"
                    subtitle="Reads the paths of SCIP indexes exported for ranking and produce path/zero-count pairs consumed by the ranking phase."
                    progress={summary.pathMapperProgress}
                />

                <Progress
                    title="Reference count mapper"
                    subtitle="Reads the symbol references of SCIP indexes exported for ranking, join them to exported definitions, and produce definition path/count pairs consumed by the ranking phase."
                    progress={summary.referenceMapperProgress}
                    className="mt-4"
                />

                {summary.reducerProgress && (
                    <Progress
                        title="Reference count reducer"
                        subtitle="Sums the references for each definition path produced by the mapping phases and groups them by repository."
                        progress={summary.reducerProgress}
                        className="mt-4"
                    />
                )}

                {onDelete && (
                    <Button variant="danger" className="p-2 mt-4" onClick={() => onDelete()}>
                        <Icon aria-hidden={true} svgPath={mdiTrashCan} /> Delete
                    </Button>
                )}
            </div>
        </Collapsible>
    </Container>
)

interface ProgressProps {
    title: string
    subtitle?: string
    progress: Progress
    className?: string
}

const Progress: FunctionComponent<ProgressProps> = ({ title, subtitle, progress, className }) => (
    <div>
        <div className={classNames(styles.tableContainer, className)}>
            <H4 className="m-0">{title}</H4>
            {subtitle && <Text size="small">{subtitle}</Text>}

            <div className={styles.row}>
                <div>Queued records</div>
                <div>
                    {progress.total === 0 ? (
                        <>No records to process</>
                    ) : (
                        <>
                            {progress.processed} of {progress.total} records processed
                        </>
                    )}
                </div>
            </div>

            <div className={styles.row}>
                <div>Progress</div>
                <div>
                    {progress.total === 0 ? 100 : Math.floor((progress.processed * 100 * 100) / progress.total) / 100}%
                </div>
            </div>

            <div className={styles.row}>
                <div>Started</div>
                <div>
                    {format(parseISO(progress.startedAt), 'MMM d y h:mm:ss a')} (
                    <Timestamp date={progress.startedAt} />)
                </div>
            </div>

            {progress.completedAt && (
                <div className={styles.row}>
                    <div>Completed</div>
                    <div>
                        {format(parseISO(progress.completedAt), 'MMM d y h:mm:ss a')} (
                        <Timestamp date={progress.completedAt} />)
                    </div>
                </div>
            )}

            {progress.completedAt && (
                <div className={styles.row}>
                    <div>Duration</div>
                    <div>Ran for {formatDistance(new Date(progress.completedAt), new Date(progress.startedAt))}</div>
                </div>
            )}
        </div>
    </div>
)
