import { FC, useCallback, useEffect, useState } from 'react'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    Button,
    Container,
    getDefaultInputProps,
    Label,
    PageHeader,
    useField,
    useForm,
    H3,
    Validator,
    ErrorAlert,
    Form,
    useDebounce,
} from '@sourcegraph/wildcard'

import {
    ConnectionContainer,
    ConnectionError,
    ConnectionForm,
    ConnectionList,
    ConnectionLoading,
    ConnectionSummary,
    ShowMoreButton,
    SummaryContainer,
} from '../../../components/FilteredConnection/ui'
import { PageTitle } from '../../../components/PageTitle'
import { RepositoriesField } from '../../insights/components'

import {
    useCancelRepoEmbeddingJob,
    useRepoEmbeddingJobsConnection,
    useScheduleContextDetectionEmbeddingJob,
    useScheduleRepoEmbeddingJobs,
} from './backend'
import { RepoEmbeddingJobNode } from './RepoEmbeddingJobNode'

import styles from './SiteAdminCodyPage.module.scss'

export interface SiteAdminCodyPageProps extends TelemetryProps {}

interface RepoEmbeddingJobsFormValues {
    repositories: string[]
}

const INITIAL_REPOSITORIES = { repositories: [] }

const repositoriesValidator: Validator<string[]> = value => {
    if (value !== undefined && value.length === 0) {
        return 'Repositories is a required field.'
    }
    return
}

export const SiteAdminCodyPage: FC<SiteAdminCodyPageProps> = ({ telemetryService }) => {
    useEffect(() => {
        telemetryService.logPageView('SiteAdminCodyPage')
    }, [telemetryService])

    const [searchValue, setSearchValue] = useState('')
    const query = useDebounce(searchValue, 200)

    const { loading, hasNextPage, fetchMore, refetchAll, connection, error } = useRepoEmbeddingJobsConnection(query)

    const [scheduleRepoEmbeddingJobs, { loading: repoEmbeddingJobsLoading, error: repoEmbeddingJobsError }] =
        useScheduleRepoEmbeddingJobs()

    const [
        scheduleContextDetectionEmbeddingJob,
        { loading: contextDetectionEmbeddingJobLoading, error: contextDetectionEmbeddingJobError },
    ] = useScheduleContextDetectionEmbeddingJob()

    const onSubmit = useCallback(
        async (repoNames: string[]) => {
            await Promise.all([
                scheduleContextDetectionEmbeddingJob(),
                scheduleRepoEmbeddingJobs({ variables: { repoNames } }),
            ])
            refetchAll()
        },
        [refetchAll, scheduleContextDetectionEmbeddingJob, scheduleRepoEmbeddingJobs]
    )

    const form = useForm<RepoEmbeddingJobsFormValues>({
        initialValues: INITIAL_REPOSITORIES,
        touched: false,
        onSubmit: values => onSubmit(values.repositories),
    })

    const repositories = useField({
        name: 'repositories',
        formApi: form.formAPI,
        validators: { sync: repositoriesValidator },
    })

    const [cancelRepoEmbeddingJob, { error: cancelRepoEmbeddingJobError }] = useCancelRepoEmbeddingJob()

    const onCancel = useCallback(
        async (id: string) => {
            await cancelRepoEmbeddingJob({ variables: { id } })
            refetchAll()
        },
        [cancelRepoEmbeddingJob, refetchAll]
    )

    return (
        <>
            <PageTitle title="Cody" />
            <PageHeader path={[{ text: 'Cody' }]} className="mb-3" headingElement="h2" />
            <Container className="mb-3">
                <H3>Schedule repositories for embedding</H3>
                <Form ref={form.ref} noValidate={true} onSubmit={form.handleSubmit}>
                    <Label htmlFor="repositories-id" className="mt-1">
                        Repositories
                    </Label>
                    <div className="d-flex">
                        <RepositoriesField
                            id="repositories-id"
                            description="Schedule repositories for embedding at latest revision on the default branch."
                            placeholder="Add repositories to schedule..."
                            className="flex-1 mr-2"
                            {...getDefaultInputProps(repositories)}
                        />
                        <div>
                            <Button
                                type="submit"
                                variant="secondary"
                                className={styles.scheduleButton}
                                disabled={repoEmbeddingJobsLoading || contextDetectionEmbeddingJobLoading}
                            >
                                {repoEmbeddingJobsLoading || contextDetectionEmbeddingJobLoading
                                    ? 'Scheduling...'
                                    : 'Schedule Embedding'}
                            </Button>
                        </div>
                    </div>
                </Form>
                {(repoEmbeddingJobsError || contextDetectionEmbeddingJobError || cancelRepoEmbeddingJobError) && (
                    <div className="mt-1">
                        <ErrorAlert
                            prefix="Error scheduling embedding jobs"
                            error={
                                repoEmbeddingJobsError ||
                                contextDetectionEmbeddingJobError ||
                                cancelRepoEmbeddingJobError
                            }
                        />
                    </div>
                )}
            </Container>
            <Container>
                <H3 className="mt-3">Repository embedding jobs</H3>
                <ConnectionContainer>
                    <ConnectionForm
                        inputValue={searchValue}
                        onInputChange={event => setSearchValue(event.target.value)}
                        inputPlaceholder="Filter embedding jobs..."
                    />
                    {error && <ConnectionError errors={[error.message]} />}
                    {loading && !connection && <ConnectionLoading />}
                    <ConnectionList as="ul" className="list-group" aria-label="Repository embedding jobs">
                        {connection?.nodes?.map(node => (
                            <RepoEmbeddingJobNode key={node.id} {...node} onCancel={onCancel} />
                        ))}
                    </ConnectionList>
                    {connection && (
                        <SummaryContainer className="mt-2" centered={true}>
                            <ConnectionSummary
                                noSummaryIfAllNodesVisible={false}
                                first={connection.totalCount ?? 0}
                                centered={true}
                                connection={connection}
                                connectionQuery={query}
                                noun="repository embedding job"
                                pluralNoun="repository embedding jobs"
                                hasNextPage={hasNextPage}
                                emptyElement={<EmptyList />}
                            />
                            {hasNextPage && <ShowMoreButton centered={true} onClick={fetchMore} />}
                        </SummaryContainer>
                    )}
                </ConnectionContainer>
            </Container>
        </>
    )
}

const EmptyList: FC<{}> = () => (
    <div className="text-muted text-center mb-3 w-100">
        <div className="pt-2">No repository embedding jobs have been created so far.</div>
    </div>
)
