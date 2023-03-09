import { FunctionComponent, useCallback, useMemo, useState } from 'react'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { useIsLightTheme } from '@sourcegraph/shared/src/theme'
import { screenReaderAnnounce, ErrorAlert, Button } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../../../../auth'
import { SaveToolbar, SaveToolbarProps, SaveToolbarPropsGenerator } from '../../../../../components/SaveToolbar'
import { DynamicallyImportedMonacoSettingsEditor } from '../../../../../settings/DynamicallyImportedMonacoSettingsEditor'
import { INFERENCE_SCRIPT } from '../../hooks/useInferenceScript'
import { useUpdateInferenceScript } from '../../hooks/useUpdateInferenceScript'

export interface InferenceScriptEditorProps extends TelemetryProps {
    script: string
    authenticatedUser: AuthenticatedUser | null
    setPreviewScript: (script: string) => void
    previewDisabled: boolean
    setTab: (index: number) => void
}

export const InferenceScriptEditor: FunctionComponent<InferenceScriptEditorProps> = ({
    script: inferenceScript,
    setPreviewScript,
    previewDisabled,
    setTab,
    authenticatedUser,
    telemetryService,
}) => {
    const { updateInferenceScript, isUpdating, updatingError } = useUpdateInferenceScript()

    const save = useCallback(
        async (script: string) =>
            updateInferenceScript({
                variables: { script },
                refetchQueries: [INFERENCE_SCRIPT],
            }).then(() => {
                screenReaderAnnounce('Saved successfully')
                setDirty(false)
            }),
        [updateInferenceScript]
    )

    const [dirty, setDirty] = useState<boolean>()
    const isLightTheme = useIsLightTheme()

    const customToolbar = useMemo<{
        saveToolbar: FunctionComponent<SaveToolbarProps>
        propsGenerator: SaveToolbarPropsGenerator<{}>
    }>(
        () => ({
            saveToolbar: props => (
                <SaveToolbar childrenPosition="start" {...props}>
                    <Button variant="success" className="mr-2" onClick={() => setTab(1)} disabled={previewDisabled}>
                        Preview
                    </Button>
                </SaveToolbar>
            ),
            propsGenerator: props => {
                const mergedProps = {
                    ...props,
                    loading: isUpdating,
                }
                mergedProps.willShowError = () => !mergedProps.saving
                mergedProps.saveDiscardDisabled = () => mergedProps.saving || !dirty

                return mergedProps
            },
        }),
        [dirty, isUpdating, previewDisabled, setTab]
    )

    return (
        <>
            {updatingError && <ErrorAlert prefix="Error saving index configuration" error={updatingError} />}
            <DynamicallyImportedMonacoSettingsEditor
                value={inferenceScript}
                language="lua"
                canEdit={authenticatedUser?.siteAdmin}
                readOnly={!authenticatedUser?.siteAdmin}
                onSave={save}
                onChange={setPreviewScript}
                saving={isUpdating}
                height={600}
                isLightTheme={isLightTheme}
                telemetryService={telemetryService}
                customSaveToolbar={authenticatedUser?.siteAdmin ? customToolbar : undefined}
                onDirtyChange={setDirty}
            />
        </>
    )
}
