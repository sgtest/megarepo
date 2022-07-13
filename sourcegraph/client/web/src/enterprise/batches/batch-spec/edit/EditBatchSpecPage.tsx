import React, { useCallback, useMemo, useState } from 'react'

import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import { useHistory } from 'react-router'

import { useQuery } from '@sourcegraph/http-client'
import { Settings, SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
// import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary/useTemporarySetting'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { Button, Icon, LoadingSpinner, H4, Alert } from '@sourcegraph/wildcard'

import { HeroPage } from '../../../../components/HeroPage'
import { useFeatureFlag } from '../../../../featureFlags/useFeatureFlag'
import {
    CheckExecutorsAccessTokenResult,
    CheckExecutorsAccessTokenVariables,
    GetBatchChangeToEditResult,
    GetBatchChangeToEditVariables,
    Scalars,
} from '../../../../graphql-operations'
// import { BatchSpecDownloadLink } from '../../BatchSpec'
import { EXECUTORS, GET_BATCH_CHANGE_TO_EDIT } from '../../create/backend'
import { ConfigurationForm } from '../../create/ConfigurationForm'
import { InsightTemplatesBanner } from '../../create/InsightTemplatesBanner'
import { SearchTemplatesBanner } from '../../create/SearchTemplatesBanner'
import { useInsightTemplates } from '../../create/useInsightTemplates'
import { useSearchTemplate } from '../../create/useSearchTemplate'
import { BatchSpecContextProvider, useBatchSpecContext, BatchSpecContextState } from '../BatchSpecContext'
import { ActionButtons } from '../header/ActionButtons'
import { BatchChangeHeader } from '../header/BatchChangeHeader'
import { TabBar, TabsConfig, TabKey } from '../TabBar'

import { DownloadSpecModal } from './DownloadSpecModal'
import { EditorFeedbackPanel } from './editor/EditorFeedbackPanel'
import { MonacoBatchSpecEditor } from './editor/MonacoBatchSpecEditor'
import { LibraryPane } from './library/LibraryPane'
import { RunBatchSpecButton } from './RunBatchSpecButton'
import { RunServerSideModal } from './RunServerSideModal'
import { WorkspacesPreviewPanel } from './workspaces-preview/WorkspacesPreviewPanel'

import layoutStyles from '../Layout.module.scss'
import styles from './EditBatchSpecPage.module.scss'

export interface EditBatchSpecPageProps extends SettingsCascadeProps<Settings>, ThemeProps {
    batchChange: { name: string; namespace: Scalars['ID'] }
}

export const EditBatchSpecPage: React.FunctionComponent<React.PropsWithChildren<EditBatchSpecPageProps>> = ({
    batchChange,
    ...props
}) => {
    const { data, error, loading, refetch } = useQuery<GetBatchChangeToEditResult, GetBatchChangeToEditVariables>(
        GET_BATCH_CHANGE_TO_EDIT,
        {
            variables: batchChange,
            // Cache this data but always re-request it in the background when we revisit
            // this page to pick up newer changes.
            fetchPolicy: 'cache-and-network',
        }
    )

    const refetchBatchChange = useCallback(() => refetch(batchChange), [refetch, batchChange])

    // If we're loading and haven't received any data yet
    if (loading && !data) {
        return (
            <div className="w-100 text-center">
                <Icon aria-label="Loading" className="m-2" as={LoadingSpinner} />
            </div>
        )
    }
    // If we received an error before we successfully received any data
    if (error && !data) {
        throw new Error(error.message)
    }
    // If there weren't any errors and we just didn't receive any data
    if (!data?.batchChange) {
        return <HeroPage icon={AlertCircleIcon} title="Batch change not found" />
    }

    // The first node from the batch specs is the latest batch spec for a batch change. If
    // it's different from the `currentSpec` on the batch change, that means the latest
    // batch spec has not yet been applied.
    const batchSpec = data.batchChange.batchSpecs.nodes[0] || data.batchChange.currentSpec

    return (
        <BatchSpecContextProvider
            batchChange={data.batchChange}
            refetchBatchChange={refetchBatchChange}
            batchSpec={batchSpec}
        >
            <EditBatchSpecPageContent {...props} />
        </BatchSpecContextProvider>
    )
}

interface EditBatchSpecPageContentProps extends SettingsCascadeProps<Settings>, ThemeProps {}

const EditBatchSpecPageContent: React.FunctionComponent<
    React.PropsWithChildren<EditBatchSpecPageContentProps>
> = props => {
    const { batchChange, batchSpec, editor, errors } = useBatchSpecContext()

    return (
        <MemoizedEditBatchSpecPageContent
            {...props}
            batchChange={batchChange}
            batchSpec={batchSpec}
            editor={editor}
            errors={errors}
        />
    )
}

type MemoizedEditBatchSpecPageContentProps = EditBatchSpecPageContentProps &
    Pick<BatchSpecContextState, 'batchChange' | 'batchSpec' | 'editor' | 'errors'>

const MemoizedEditBatchSpecPageContent: React.FunctionComponent<
    React.PropsWithChildren<MemoizedEditBatchSpecPageContentProps>
> = React.memo(function MemoizedEditBatchSpecPageContent({
    settingsCascade,
    isLightTheme,
    batchChange,
    batchSpec,
    editor,
    errors,
}) {
    const history = useHistory()

    const { insightTitle } = useInsightTemplates(settingsCascade)
    const { searchQuery } = useSearchTemplate()

    const [activeTabKey, setActiveTabKey] = useState<TabKey>('spec')
    const tabsConfig = useMemo<TabsConfig[]>(
        () => [
            {
                key: 'configuration',
                isEnabled: true,
                handler: {
                    type: 'button',
                    onClick: () => setActiveTabKey('configuration'),
                },
            },
            {
                key: 'spec',
                isEnabled: true,
                handler: {
                    type: 'button',
                    onClick: () => setActiveTabKey('spec'),
                },
            },
        ],
        []
    )

    // Check for active executors to tell if we are able to run batch changes server-side.
    const { data } = useQuery<CheckExecutorsAccessTokenResult, CheckExecutorsAccessTokenVariables>(EXECUTORS, {})

    const [isDownloadSpecModalOpen, setIsDownloadSpecModalOpen] = useState(false)
    const [isRunServerSideModalOpen, setIsRunServerSideModalOpen] = useState(false)
    // NOTE: Uncomment these lines to restore "Don't show this again" functionality for
    // "Download spec for src-cli" modal.
    // const [downloadSpecModalDismissed, setDownloadSpecModalDismissed] = useTemporarySetting(
    //     'batches.downloadSpecModalDismissed',
    //     false
    // )

    /**
     * For managed instances we want to hide the `run server side` button by default. To do this we make use of a
     * feature flag to ensure Managed Instances.
     */
    const [isRunBatchSpecButtonHidden] = useFeatureFlag('hide-run-batch-spec-for-mi', false)

    const activeExecutorsActionButtons = (
        <>
            <RunBatchSpecButton
                execute={editor.execute}
                isExecutionDisabled={editor.isExecutionDisabled}
                options={editor.executionOptions}
                onChangeOptions={editor.setExecutionOptions}
            />
            {/* NOTE: Uncomment these lines to restore "Don't show this again" functionality
            for "Download spec for src-cli" modal.
            {downloadSpecModalDismissed ? (
                <BatchSpecDownloadLink
                    name={batchChange.name}
                    originalInput={editor.code}
                    isLightTheme={isLightTheme}
                    asButton={false}
                >
                    or download for src-cli
                </BatchSpecDownloadLink>
            ) : ( */}
            <Button className={styles.downloadLink} variant="link" onClick={() => setIsDownloadSpecModalOpen(true)}>
                or download for src-cli
            </Button>
            {/* )} */}
        </>
    )

    const noActiveExecutorsActionButtons = (
        <>
            {/* NOTE: Uncomment these lines to restore "Don't show this again" functionality
            for "Download spec for src-cli" modal.
            {downloadSpecModalDismissed ? (
                <BatchSpecDownloadLink
                    name={batchChange.name}
                    originalInput={editor.code}
                    isLightTheme={isLightTheme}
                    asButton={true}
                    className="mb-2"
                >
                    Download for src-cli
                </BatchSpecDownloadLink>
            ) : ( */}
            <Button className="mb-2" variant="primary" onClick={() => setIsDownloadSpecModalOpen(true)}>
                Download for src-cli
            </Button>
            {/* )} */}

            {!isRunBatchSpecButtonHidden && (
                <Button
                    className={styles.downloadLink}
                    variant="link"
                    onClick={() => setIsRunServerSideModalOpen(true)}
                >
                    or run server-side
                </Button>
            )}
        </>
    )

    // When graphql query is completed, check if the data from the query meets this condition and render approriate buttons
    // Until the query is complete, this variable will be undefined and no buttons will show
    const actionButtons = data
        ? data.areExecutorsConfigured
            ? activeExecutorsActionButtons
            : noActiveExecutorsActionButtons
        : undefined

    const executionAlert = batchSpec.isExecuting ? (
        <Alert variant="warning" className="d-flex align-items-center pr-3">
            <div className="flex-grow-1 pr-3">
                <H4>There is another active execution for this batch change.</H4>
                You're about to edit a batch spec that is currently being executed. You might want to view or cancel
                that execution first.
            </div>
            <Button variant="primary" onClick={() => history.replace(`${batchChange.url}/executions/${batchSpec.id}`)}>
                Go to execution
            </Button>
        </Alert>
    ) : null

    return (
        <div className={layoutStyles.pageContainer}>
            {searchQuery && <SearchTemplatesBanner className="mb-3" />}
            {insightTitle && <InsightTemplatesBanner insightTitle={insightTitle} type="create" className="mb-3" />}
            <div className={layoutStyles.headerContainer}>
                <BatchChangeHeader
                    namespace={{
                        to: `${batchChange.namespace.url}/batch-changes`,
                        text: batchChange.namespace.namespaceName,
                    }}
                    title={{ to: batchChange.url, text: batchChange.name }}
                    description={batchChange.description ?? undefined}
                />
                <ActionButtons>{actionButtons}</ActionButtons>
            </div>
            <TabBar activeTabKey={activeTabKey} tabsConfig={tabsConfig} />

            {activeTabKey === 'configuration' ? (
                <ConfigurationForm isReadOnly={true} batchChange={batchChange} settingsCascade={settingsCascade} />
            ) : (
                <div className={styles.form}>
                    <LibraryPane name={batchChange.name} onReplaceItem={editor.handleCodeChange} />
                    <div className={styles.editorContainer}>
                        <H4 className={styles.header}>Batch spec</H4>
                        {executionAlert}
                        <MonacoBatchSpecEditor
                            autoFocus={true}
                            batchChangeName={batchChange.name}
                            className={styles.editor}
                            isLightTheme={isLightTheme}
                            value={editor.code}
                            onChange={editor.handleCodeChange}
                        />
                        <EditorFeedbackPanel errors={errors} />
                    </div>
                    <WorkspacesPreviewPanel />
                </div>
            )}

            {isDownloadSpecModalOpen ? (
                <DownloadSpecModal
                    name={batchChange.name}
                    originalInput={editor.code}
                    isLightTheme={isLightTheme}
                    // NOTE: Uncomment this line to restore "Don't show this again"
                    // functionality for "Download spec for src-cli" modal.
                    // setDownloadSpecModalDismissed={setDownloadSpecModalDismissed}
                    setIsDownloadSpecModalOpen={setIsDownloadSpecModalOpen}
                />
            ) : null}
            {isRunServerSideModalOpen ? (
                <RunServerSideModal setIsRunServerSideModalOpen={setIsRunServerSideModalOpen} />
            ) : null}
        </div>
    )
})
