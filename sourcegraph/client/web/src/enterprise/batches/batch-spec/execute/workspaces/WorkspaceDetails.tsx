import React, { useCallback, useMemo, useState } from 'react'

import VisuallyHidden from '@reach/visually-hidden'
import classNames from 'classnames'
import { cloneDeep } from 'lodash'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import CheckBoldIcon from 'mdi-react/CheckBoldIcon'
import CloseIcon from 'mdi-react/CloseIcon'
import ContentSaveIcon from 'mdi-react/ContentSaveIcon'
import ExternalLinkIcon from 'mdi-react/ExternalLinkIcon'
import EyeOffOutlineIcon from 'mdi-react/EyeOffOutlineIcon'
import LinkVariantRemoveIcon from 'mdi-react/LinkVariantRemoveIcon'
import MapSearchIcon from 'mdi-react/MapSearchIcon'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'
import SyncIcon from 'mdi-react/SyncIcon'
import TimelineClockOutlineIcon from 'mdi-react/TimelineClockOutlineIcon'
import TimerSandIcon from 'mdi-react/TimerSandIcon'
import indicator from 'ordinal/indicator'
import { useHistory } from 'react-router'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { Maybe } from '@sourcegraph/shared/src/graphql-operations'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import {
    Badge,
    LoadingSpinner,
    Tab,
    TabList,
    TabPanel,
    TabPanels,
    Tabs,
    Button,
    Link,
    CardBody,
    Card,
    Icon,
    Code,
    H1,
    H3,
    H4,
    Text,
} from '@sourcegraph/wildcard'

import { Collapsible } from '../../../../../components/Collapsible'
import { DiffStat } from '../../../../../components/diff/DiffStat'
import { FileDiffConnection } from '../../../../../components/diff/FileDiffConnection'
import { FileDiffNode } from '../../../../../components/diff/FileDiffNode'
import { FilteredConnectionQueryArguments } from '../../../../../components/FilteredConnection'
import { HeroPage } from '../../../../../components/HeroPage'
import { LogOutput } from '../../../../../components/LogOutput'
import { Duration } from '../../../../../components/time/Duration'
import {
    BatchSpecWorkspaceChangesetSpecFields,
    BatchSpecWorkspaceState,
    BatchSpecWorkspaceStepFields,
    HiddenBatchSpecWorkspaceFields,
    Scalars,
    VisibleBatchSpecWorkspaceFields,
} from '../../../../../graphql-operations'
import { queryChangesetSpecFileDiffs as _queryChangesetSpecFileDiffs } from '../../../preview/list/backend'
import { ChangesetSpecFileDiffConnection } from '../../../preview/list/ChangesetSpecFileDiffConnection'
import {
    useBatchSpecWorkspace,
    useRetryWorkspaceExecution,
    queryBatchSpecWorkspaceStepFileDiffs as _queryBatchSpecWorkspaceStepFileDiffs,
} from '../backend'
import { TimelineModal } from '../TimelineModal'

import { WorkspaceStateIcon } from './WorkspaceStateIcon'

import styles from './WorkspaceDetails.module.scss'

export interface WorkspaceDetailsProps extends ThemeProps {
    id: Scalars['ID']
    /** Handler to deselect the current workspace, i.e. close the details panel. */
    deselectWorkspace?: () => void
    /** For testing purposes only */
    queryBatchSpecWorkspaceStepFileDiffs?: typeof _queryBatchSpecWorkspaceStepFileDiffs
    queryChangesetSpecFileDiffs?: typeof _queryChangesetSpecFileDiffs
}

export const WorkspaceDetails: React.FunctionComponent<React.PropsWithChildren<WorkspaceDetailsProps>> = ({
    id,
    ...props
}) => {
    // Fetch and poll latest workspace information.
    const { loading, error, data } = useBatchSpecWorkspace(id)

    // If we're loading and haven't received any data yet
    if (loading && !data) {
        return <LoadingSpinner />
    }
    // If we received an error before we had received any data
    if (error && !data) {
        return <ErrorAlert error={error} />
    }
    // If there weren't any errors and we just didn't receive any data
    if (!data) {
        return <HeroPage icon={MapSearchIcon} title="404: Not Found" />
    }

    const workspace = data

    if (workspace.__typename === 'HiddenBatchSpecWorkspace') {
        return <HiddenWorkspaceDetails {...props} workspace={workspace} />
    }
    return <VisibleWorkspaceDetails {...props} workspace={workspace} />
}

interface WorkspaceHeaderProps extends Pick<WorkspaceDetailsProps, 'deselectWorkspace'> {
    workspace: HiddenBatchSpecWorkspaceFields | VisibleBatchSpecWorkspaceFields
    toggleShowTimeline?: () => void
}

const WorkspaceHeader: React.FunctionComponent<React.PropsWithChildren<WorkspaceHeaderProps>> = ({
    workspace,
    deselectWorkspace,
    toggleShowTimeline,
}) => (
    <>
        <div className="d-flex align-items-center justify-content-between mb-2">
            <H3 className={styles.workspaceName}>
                <WorkspaceStateIcon
                    cachedResultFound={workspace.cachedResultFound}
                    state={workspace.state}
                    className="flex-shrink-0"
                />{' '}
                {workspace.__typename === 'VisibleBatchSpecWorkspace'
                    ? workspace.repository.name
                    : 'Workspace in hidden repository'}
                {workspace.__typename === 'VisibleBatchSpecWorkspace' && (
                    <Link to={workspace.repository.url} target="_blank" rel="noopener noreferrer">
                        <Icon aria-hidden={true} as={ExternalLinkIcon} />
                    </Link>
                )}
            </H3>
            <Button className="p-0 ml-2" onClick={deselectWorkspace} variant="icon">
                <VisuallyHidden>Deselect Workspace</VisuallyHidden>
                <Icon aria-hidden={true} as={CloseIcon} />
            </Button>
        </div>
        <div className="d-flex align-items-center">
            {typeof workspace.placeInQueue === 'number' && (
                <span
                    className={classNames(styles.workspaceDetail, 'd-flex align-items-center')}
                    data-tooltip={`This workspace is number ${workspace.placeInGlobalQueue} in the global queue`}
                >
                    <Icon aria-hidden={true} as={TimelineClockOutlineIcon} />
                    <strong className="ml-1 mr-1">
                        <NumberInQueue number={workspace.placeInQueue} />
                    </strong>
                    in queue
                </span>
            )}
            {workspace.__typename === 'VisibleBatchSpecWorkspace' && workspace.path && (
                <span className={styles.workspaceDetail}>{workspace.path}</span>
            )}
            {workspace.__typename === 'VisibleBatchSpecWorkspace' && (
                <span className={styles.workspaceDetail}>
                    <Icon aria-hidden={true} as={SourceBranchIcon} /> {workspace.branch.displayName}
                </span>
            )}
            {workspace.startedAt && (
                <span className={styles.workspaceDetail}>
                    Total time:{' '}
                    <strong>
                        <Duration start={workspace.startedAt} end={workspace.finishedAt ?? undefined} />
                    </strong>
                </span>
            )}
            {toggleShowTimeline && !workspace.cachedResultFound && workspace.state !== BatchSpecWorkspaceState.SKIPPED && (
                <Button className={styles.workspaceDetail} onClick={toggleShowTimeline} variant="link">
                    Timeline
                </Button>
            )}
        </div>
        <hr className="mb-3" />
    </>
)

interface HiddenWorkspaceDetailsProps extends Pick<WorkspaceDetailsProps, 'deselectWorkspace'> {
    workspace: HiddenBatchSpecWorkspaceFields
}

const HiddenWorkspaceDetails: React.FunctionComponent<React.PropsWithChildren<HiddenWorkspaceDetailsProps>> = ({
    workspace,
    deselectWorkspace,
}) => (
    <>
        <WorkspaceHeader deselectWorkspace={deselectWorkspace} workspace={workspace} />
        <H1 className="text-center text-muted mt-5">
            <Icon aria-hidden={true} as={EyeOffOutlineIcon} />
            <VisuallyHidden>Hidden Workspace</VisuallyHidden>
        </H1>
        <Text alignment="center">This workspace is hidden due to permissions.</Text>
        <Text alignment="center">Contact the owner of this batch change for more information.</Text>
    </>
)

interface VisibleWorkspaceDetailsProps extends Omit<WorkspaceDetailsProps, 'id'> {
    workspace: VisibleBatchSpecWorkspaceFields
}

const VisibleWorkspaceDetails: React.FunctionComponent<React.PropsWithChildren<VisibleWorkspaceDetailsProps>> = ({
    isLightTheme,
    workspace,
    deselectWorkspace,
    queryBatchSpecWorkspaceStepFileDiffs,
    queryChangesetSpecFileDiffs,
}) => {
    const [retryWorkspaceExecution, { loading: retryLoading, error: retryError }] = useRetryWorkspaceExecution(
        workspace.id
    )

    const [showTimeline, setShowTimeline] = useState<boolean>(false)
    const toggleShowTimeline = useCallback(() => {
        setShowTimeline(true)
    }, [])
    const onDismissTimeline = useCallback(() => {
        setShowTimeline(false)
    }, [])

    if (workspace.state === BatchSpecWorkspaceState.SKIPPED && workspace.ignored) {
        return <IgnoredWorkspaceDetails workspace={workspace} deselectWorkspace={deselectWorkspace} />
    }

    if (workspace.state === BatchSpecWorkspaceState.SKIPPED && workspace.unsupported) {
        return <UnsupportedWorkspaceDetails workspace={workspace} deselectWorkspace={deselectWorkspace} />
    }

    return (
        <>
            {showTimeline && <TimelineModal node={workspace} onCancel={onDismissTimeline} />}
            <WorkspaceHeader
                deselectWorkspace={deselectWorkspace}
                toggleShowTimeline={toggleShowTimeline}
                workspace={workspace}
            />
            {workspace.failureMessage && (
                <>
                    <div className="d-flex my-3 w-100">
                        <ErrorAlert error={workspace.failureMessage} className="flex-grow-1 mb-0" />
                        <Button
                            className="ml-2"
                            onClick={() => retryWorkspaceExecution()}
                            disabled={retryLoading}
                            outline={true}
                            variant="danger"
                        >
                            <Icon aria-hidden={true} as={SyncIcon} /> Retry
                        </Button>
                    </div>
                    {retryError && <ErrorAlert error={retryError} />}
                </>
            )}

            {workspace.changesetSpecs && workspace.state === BatchSpecWorkspaceState.COMPLETED && (
                <div className="mb-3">
                    {workspace.changesetSpecs.length === 0 && (
                        <Text className="mb-0 text-muted">This workspace generated no changeset specs.</Text>
                    )}
                    {workspace.changesetSpecs.map((changesetSpec, index) => (
                        <React.Fragment key={changesetSpec.id}>
                            <ChangesetSpecNode
                                node={changesetSpec}
                                isLightTheme={isLightTheme}
                                queryChangesetSpecFileDiffs={queryChangesetSpecFileDiffs}
                            />
                            {index !== workspace.changesetSpecs!.length - 1 && <hr className="m-0" />}
                        </React.Fragment>
                    ))}
                </div>
            )}

            {workspace.steps.map((step, index) => (
                <React.Fragment key={step.number}>
                    <WorkspaceStep
                        step={step}
                        cachedResultFound={workspace.cachedResultFound}
                        workspaceID={workspace.id}
                        isLightTheme={isLightTheme}
                        queryBatchSpecWorkspaceStepFileDiffs={queryBatchSpecWorkspaceStepFileDiffs}
                    />
                    {index !== workspace.steps.length - 1 && <hr className="my-2" />}
                </React.Fragment>
            ))}
        </>
    )
}

interface IgnoredWorkspaceDetailsProps extends Pick<WorkspaceDetailsProps, 'deselectWorkspace'> {
    workspace: VisibleBatchSpecWorkspaceFields
}

const IgnoredWorkspaceDetails: React.FunctionComponent<React.PropsWithChildren<IgnoredWorkspaceDetailsProps>> = ({
    workspace,
    deselectWorkspace,
}) => (
    <>
        <WorkspaceHeader deselectWorkspace={deselectWorkspace} workspace={workspace} />
        <H1 className="text-center text-muted mt-5">
            <Icon aria-hidden={true} as={LinkVariantRemoveIcon} />
            <VisuallyHidden>Ignored Workspace</VisuallyHidden>
        </H1>
        <Text alignment="center">
            This workspace has been skipped because a <Code>.batchignore</Code> file is present in the workspace
            repository.
        </Text>
        <Text alignment="center">Enable the execution option to "allow ignored" to override.</Text>
    </>
)

interface UnsupportedWorkspaceDetailsProps extends Pick<WorkspaceDetailsProps, 'deselectWorkspace'> {
    workspace: VisibleBatchSpecWorkspaceFields
}

const UnsupportedWorkspaceDetails: React.FunctionComponent<
    React.PropsWithChildren<UnsupportedWorkspaceDetailsProps>
> = ({ workspace, deselectWorkspace }) => (
    <>
        <WorkspaceHeader deselectWorkspace={deselectWorkspace} workspace={workspace} />
        <H1 className="text-center text-muted mt-5">
            <Icon aria-hidden={true} as={LinkVariantRemoveIcon} />
            <VisuallyHidden>Unsupported Workspace</VisuallyHidden>
        </H1>
        <Text alignment="center">This workspace has been skipped because it is from an unsupported codehost.</Text>
        <Text alignment="center">Enable the execution option to "allow unsupported" to override.</Text>
    </>
)

const NumberInQueue: React.FunctionComponent<React.PropsWithChildren<{ number: number }>> = ({ number }) => (
    <>
        {number}
        <sup>{indicator(number)}</sup>
    </>
)

interface ChangesetSpecNodeProps extends ThemeProps {
    node: BatchSpecWorkspaceChangesetSpecFields
    queryChangesetSpecFileDiffs?: typeof _queryChangesetSpecFileDiffs
}

const ChangesetSpecNode: React.FunctionComponent<React.PropsWithChildren<ChangesetSpecNodeProps>> = ({
    node,
    isLightTheme,
    queryChangesetSpecFileDiffs = _queryChangesetSpecFileDiffs,
}) => {
    const history = useHistory()

    // TODO: This should not happen. When the workspace is visibile, the changeset spec should be visible as well.
    if (node.__typename === 'HiddenChangesetSpec') {
        return (
            <Card>
                <CardBody>
                    <H4>Changeset in a hidden repo</H4>
                </CardBody>
            </Card>
        )
    }

    // This should not happen.
    if (node.description.__typename === 'ExistingChangesetReference') {
        return null
    }

    return (
        <Collapsible
            title={
                <div className="d-flex justify-content-between">
                    <div>
                        <H4 className="mb-0 d-inline-block mr-2">
                            <H3 className={styles.result}>Result</H3>
                            {node.description.published !== null && (
                                <Badge className="text-uppercase">
                                    {publishBadgeLabel(node.description.published)}
                                </Badge>
                            )}{' '}
                        </H4>
                        <span className="text-muted">
                            <Icon aria-hidden={true} as={SourceBranchIcon} /> {node.description.headRef}
                        </span>
                    </div>
                    <DiffStat {...node.description.diffStat} expandedCounts={true} />
                </div>
            }
            titleClassName="flex-grow-1"
            // TODO: Under what conditions should this be auto-expanded?
            defaultExpanded={true}
        >
            <Card className={classNames('mt-2', styles.resultCard)}>
                <CardBody>
                    <H3>Changeset template</H3>
                    <H4>{node.description.title}</H4>
                    <Text className="mb-0">{node.description.body}</Text>
                    <Text>
                        <strong>Published:</strong> <PublishedValue published={node.description.published} />
                    </Text>
                    <Collapsible
                        title={<H3 className="mb-0">Changes</H3>}
                        titleClassName="flex-grow-1"
                        defaultExpanded={true}
                    >
                        <ChangesetSpecFileDiffConnection
                            history={history}
                            isLightTheme={isLightTheme}
                            location={history.location}
                            spec={node.id}
                            queryChangesetSpecFileDiffs={queryChangesetSpecFileDiffs}
                        />
                    </Collapsible>
                </CardBody>
            </Card>
        </Collapsible>
    )
}

function publishBadgeLabel(state: Scalars['PublishedValue']): string {
    switch (state) {
        case 'draft':
            return 'will publish as draft'
        case false:
            return 'will not publish'
        case true:
            return 'will publish'
    }
}

const PublishedValue: React.FunctionComponent<
    React.PropsWithChildren<{ published: Scalars['PublishedValue'] | null }>
> = ({ published }) => {
    if (published === null) {
        return <i>select from UI when applying</i>
    }
    if (published === 'draft') {
        return <>draft</>
    }
    return <>{String(published)}</>
}

interface WorkspaceStepProps extends ThemeProps {
    cachedResultFound: boolean
    step: BatchSpecWorkspaceStepFields
    workspaceID: Scalars['ID']
    /** For testing purposes only */
    queryBatchSpecWorkspaceStepFileDiffs?: typeof _queryBatchSpecWorkspaceStepFileDiffs
}

const WorkspaceStep: React.FunctionComponent<React.PropsWithChildren<WorkspaceStepProps>> = ({
    step,
    isLightTheme,
    workspaceID,
    cachedResultFound,
    queryBatchSpecWorkspaceStepFileDiffs,
}) => {
    const outputLines = useMemo(() => {
        const outputLines = cloneDeep(step.outputLines)
        if (outputLines !== null) {
            if (
                outputLines.every(
                    line =>
                        line
                            .replaceAll(/'^std(out|err):'/g, '')
                            .replaceAll('\n', '')
                            .trim() === ''
                )
            ) {
                outputLines.push('stderr: This command did not produce any logs')
            }
            if (step.exitCode !== null) {
                outputLines.push(`\nstdout: \nstdout: Command exited with status ${step.exitCode}`)
            }
        }
        return outputLines
    }, [step.exitCode, step.outputLines])

    return (
        <Collapsible
            titleClassName={styles.collapsible}
            title={
                <>
                    <div className={classNames(styles.stepHeader, step.skipped && 'text-muted')}>
                        <StepStateIcon step={step} />
                        <H3 className={styles.stepNumber}>Step {step.number}</H3>
                        <span className={classNames('text-monospace text-muted', styles.stepCommand)}>{step.run}</span>
                    </div>
                    {step.diffStat && (
                        <DiffStat className={styles.stepDiffStat} {...step.diffStat} expandedCounts={true} />
                    )}
                    {step.startedAt && (
                        <span className={classNames('text-monospace text-muted', styles.stepTime)}>
                            <StepTimer startedAt={step.startedAt} finishedAt={step.finishedAt} />
                        </span>
                    )}
                </>
            }
        >
            <Card className={classNames('mt-2', styles.stepCard)}>
                <CardBody>
                    {!step.skipped && (
                        <Tabs size="small" behavior="forceRender">
                            <TabList>
                                <Tab key="logs">Logs</Tab>
                                <Tab key="output-variables">Output variables</Tab>
                                <Tab key="diff">Diff</Tab>
                                <Tab key="files-env">Files / Env</Tab>
                                <Tab key="command-container">Commands / Container</Tab>
                            </TabList>
                            <TabPanels>
                                <TabPanel className="pt-2" key="logs">
                                    {!step.startedAt && <Text className="text-muted mb-0">Step not started yet</Text>}
                                    {step.startedAt && outputLines && <LogOutput text={outputLines.join('\n')} />}
                                </TabPanel>
                                <TabPanel className="pt-2" key="output-variables">
                                    {!step.startedAt && <Text className="text-muted mb-0">Step not started yet</Text>}
                                    {step.outputVariables?.length === 0 && (
                                        <Text className="text-muted mb-0">No output variables specified</Text>
                                    )}
                                    <ul className="mb-0">
                                        {step.outputVariables?.map(variable => (
                                            <li key={variable.name}>
                                                {variable.name}: {variable.value}
                                            </li>
                                        ))}
                                    </ul>
                                </TabPanel>
                                <TabPanel className="pt-2" key="diff">
                                    {!step.startedAt && <Text className="text-muted mb-0">Step not started yet</Text>}
                                    {step.startedAt && (
                                        <WorkspaceStepFileDiffConnection
                                            isLightTheme={isLightTheme}
                                            step={step.number}
                                            workspaceID={workspaceID}
                                            queryBatchSpecWorkspaceStepFileDiffs={queryBatchSpecWorkspaceStepFileDiffs}
                                        />
                                    )}
                                </TabPanel>
                                <TabPanel className="pt-2" key="files-env">
                                    {step.environment.length === 0 && (
                                        <Text className="text-muted mb-0">No environment variables specified</Text>
                                    )}
                                    <ul className="mb-0">
                                        {step.environment.map(variable => (
                                            <li key={variable.name}>
                                                {variable.name}: {variable.value}
                                            </li>
                                        ))}
                                    </ul>
                                </TabPanel>
                                <TabPanel className="pt-2" key="command-container">
                                    {step.ifCondition !== null && (
                                        <>
                                            <H4>If condition</H4>
                                            <LogOutput text={step.ifCondition} className="mb-2" />
                                        </>
                                    )}
                                    <H4>Command</H4>
                                    <LogOutput text={step.run} className="mb-2" />
                                    <H4>Container</H4>
                                    <Text className="text-monospace mb-0">{step.container}</Text>
                                </TabPanel>
                            </TabPanels>
                        </Tabs>
                    )}
                    {step.skipped && (
                        <Text className="mb-0">
                            <strong>
                                Step has been skipped
                                {cachedResultFound && <> because a cached result was found for this workspace</>}
                                {!cachedResultFound && step.cachedResultFound && (
                                    <> because a cached result was found for this step</>
                                )}
                                .
                            </strong>
                        </Text>
                    )}
                </CardBody>
            </Card>
        </Collapsible>
    )
}

interface StepStateIconProps {
    step: BatchSpecWorkspaceStepFields
}
const StepStateIcon: React.FunctionComponent<React.PropsWithChildren<StepStateIconProps>> = ({ step }) => {
    if (step.cachedResultFound) {
        return (
            <Icon
                className="text-success flex-shrink-0"
                aria-label="A cached result for this step has been found"
                data-tooltip="A cached result for this step has been found"
                as={ContentSaveIcon}
            />
        )
    }
    if (step.skipped) {
        return (
            <Icon
                className="text-muted flex-shrink-0"
                aria-label="The step has been skipped"
                data-tooltip="The step has been skipped"
                as={LinkVariantRemoveIcon}
            />
        )
    }
    if (!step.startedAt) {
        return (
            <Icon
                className="text-muted flex-shrink-0"
                aria-label="This step is waiting to be processed"
                data-tooltip="This step is waiting to be processed"
                as={TimerSandIcon}
            />
        )
    }
    if (!step.finishedAt) {
        return (
            <Icon
                className="text-muted flex-shrink-0"
                aria-label="This step is currently running"
                data-tooltip="This step is currently running"
                as={LoadingSpinner}
            />
        )
    }
    if (step.exitCode === 0) {
        return (
            <Icon
                className="text-success flex-shrink-0"
                aria-label="This step ran successfully"
                data-tooltip="This step ran successfully"
                as={CheckBoldIcon}
            />
        )
    }
    return (
        <Icon
            className="text-danger flex-shrink-0"
            aria-label={`This step failed with exit code ${String(step.exitCode)}`}
            data-tooltip={`This step failed with exit code ${String(step.exitCode)}`}
            as={AlertCircleIcon}
        />
    )
}

const StepTimer: React.FunctionComponent<React.PropsWithChildren<{ startedAt: string; finishedAt: Maybe<string> }>> = ({
    startedAt,
    finishedAt,
}) => <Duration start={startedAt} end={finishedAt ?? undefined} />

interface WorkspaceStepFileDiffConnectionProps extends ThemeProps {
    workspaceID: Scalars['ID']
    step: number
    queryBatchSpecWorkspaceStepFileDiffs?: typeof _queryBatchSpecWorkspaceStepFileDiffs
}

const WorkspaceStepFileDiffConnection: React.FunctionComponent<
    React.PropsWithChildren<WorkspaceStepFileDiffConnectionProps>
> = ({
    workspaceID,
    step,
    isLightTheme,
    queryBatchSpecWorkspaceStepFileDiffs = _queryBatchSpecWorkspaceStepFileDiffs,
}) => {
    const queryFileDiffs = useCallback(
        (args: FilteredConnectionQueryArguments) =>
            queryBatchSpecWorkspaceStepFileDiffs({
                after: args.after ?? null,
                first: args.first ?? null,
                node: workspaceID,
                step,
            }),
        [workspaceID, step, queryBatchSpecWorkspaceStepFileDiffs]
    )
    const history = useHistory()
    return (
        <FileDiffConnection
            listClassName="list-group list-group-flush"
            noun="changed file"
            pluralNoun="changed files"
            queryConnection={queryFileDiffs}
            nodeComponent={FileDiffNode}
            nodeComponentProps={{
                history,
                location: history.location,
                isLightTheme,
                persistLines: true,
                lineNumbers: true,
            }}
            defaultFirst={15}
            hideSearch={true}
            noSummaryIfAllNodesVisible={true}
            history={history}
            location={history.location}
            useURLQuery={false}
            cursorPaging={true}
        />
    )
}
