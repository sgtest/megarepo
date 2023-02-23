import * as React from 'react'

import * as _monaco from 'monaco-editor' // type only
import { Subscription } from 'rxjs'

import { logger } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { LoadingSpinner, BeforeUnloadPrompt } from '@sourcegraph/wildcard'

import { SaveToolbarProps, SaveToolbar, SaveToolbarPropsGenerator } from '../components/SaveToolbar'

import { EditorAction, EditorActionsGroup } from './EditorActionsGroup'
import * as _monacoSettingsEditorModule from './MonacoSettingsEditor'

/**
 * Converts a Monaco/vscode style Disposable object to a simple function that can be added to a rxjs Subscription
 */
const disposableToFunc = (disposable: _monaco.IDisposable) => () => disposable.dispose()

interface Props<T extends object>
    extends Pick<_monacoSettingsEditorModule.Props, 'id' | 'readOnly' | 'height' | 'jsonSchema' | 'language'>,
        TelemetryProps {
    value: string
    isLightTheme: boolean

    actions?: EditorAction[]

    loading?: boolean
    saving?: boolean

    canEdit?: boolean

    className?: string

    /**
     * Block navigation if the editor contents were changed.
     * Set to `false` if another component already blocks navigation.
     *
     * @default true
     */
    blockNavigationIfDirty?: boolean

    onSave?: (value: string) => Promise<string | void>
    onChange?: (value: string) => void
    onDirtyChange?: (dirty: boolean) => void
    onEditor?: (editor: _monaco.editor.ICodeEditor) => void

    customSaveToolbar?: {
        propsGenerator: SaveToolbarPropsGenerator<T & { children?: React.ReactNode }>
        saveToolbar: React.FunctionComponent<React.PropsWithChildren<SaveToolbarProps & T>>
    }

    explanation?: JSX.Element
}

interface State {
    /** The current contents of the editor, if changed from Props.value. */
    value?: string
}

const MonacoSettingsEditor = React.lazy(async () => ({
    default: (await import('./MonacoSettingsEditor')).MonacoSettingsEditor,
}))

/** Displays a MonacoSettingsEditor component without loading Monaco in the current Webpack chunk. */
export class DynamicallyImportedMonacoSettingsEditor<T extends object = {}> extends React.PureComponent<
    Props<T>,
    State
> {
    public state: State = {}

    private subscriptions = new Subscription()

    private monaco: typeof _monaco | null = null
    private configEditor?: _monaco.editor.ICodeEditor

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    private get effectiveValue(): string {
        return this.state.value === undefined ? this.props.value : this.state.value
    }

    private get isDirty(): boolean {
        return this.effectiveValue !== this.props.value
    }

    public render(): JSX.Element | null {
        const effectiveValue = this.effectiveValue

        let saveToolbar: React.ReactElement | null = null
        if (this.props.customSaveToolbar) {
            const toolbarProps = this.props.customSaveToolbar.propsGenerator({
                dirty: this.isDirty,
                saving: this.props.saving,
                onSave: this.onSave,
                onDiscard: this.discard,
            })
            saveToolbar = this.props.customSaveToolbar.saveToolbar(toolbarProps)
        } else {
            saveToolbar = (
                <SaveToolbar
                    dirty={this.isDirty}
                    saving={this.props.saving}
                    onSave={this.onSave}
                    onDiscard={this.discard}
                />
            )
        }

        const { className, blockNavigationIfDirty, ...otherProps } = this.props

        return (
            <div className={className || ''}>
                {blockNavigationIfDirty && (
                    <BeforeUnloadPrompt when={this.props.loading || this.isDirty} message="Discard changes?" />
                )}
                {this.props.actions && (
                    <EditorActionsGroup actions={this.props.actions} onClick={this.runAction.bind(this)} />
                )}
                <React.Suspense fallback={<LoadingSpinner className="mt-2" />}>
                    <MonacoSettingsEditor
                        {...otherProps}
                        onDidSave={this.onSave}
                        onChange={this.onChange}
                        value={effectiveValue}
                        monacoRef={this.monacoRef}
                    />
                </React.Suspense>
                {this.props.explanation && this.props.explanation}
                {this.props.canEdit && saveToolbar}
            </div>
        )
    }

    private onSave = async (): Promise<void> => {
        const value = this.effectiveValue
        if (this.props.onSave) {
            const newConfig = await this.props.onSave(value)
            if (newConfig) {
                this.setState({ value: newConfig })
            }
        }
    }

    private onChange = (newValue: string): void => {
        this.setState({ value: newValue }, () => {
            if (this.props.onChange) {
                this.props.onChange(newValue)
            }
            if (this.props.onDirtyChange) {
                this.props.onDirtyChange(this.isDirty)
            }
        })
    }

    private discard = (): void => {
        if (
            this.state.value === undefined ||
            this.props.value === this.state.value ||
            window.confirm('Discard edits?')
        ) {
            this.setState({ value: undefined })
        }
    }

    private monacoRef = (monacoValue: typeof _monaco | null): void => {
        this.monaco = monacoValue
        if (this.monaco) {
            this.subscriptions.add(
                disposableToFunc(
                    this.monaco.editor.onDidCreateEditor(editor => {
                        this.configEditor = editor
                        this.props.onEditor?.(editor)
                    })
                )
            )
            this.subscriptions.add(
                disposableToFunc(
                    this.monaco.editor.onDidCreateModel(async model => {
                        // This function can only be called if the lazy MonacoSettingsEditor component was loaded,
                        // so this import call will not incur another load.
                        const { MonacoSettingsEditor } = await import('./MonacoSettingsEditor')

                        if (
                            this.configEditor &&
                            MonacoSettingsEditor.isStandaloneCodeEditor(this.configEditor) &&
                            this.props.actions
                        ) {
                            for (const { id, label, run } of this.props.actions) {
                                MonacoSettingsEditor.addEditorAction(
                                    this.configEditor,
                                    model,
                                    label,
                                    id,
                                    run,
                                    this.props.telemetryService
                                )
                            }
                        }
                    })
                )
            )
        }
    }

    private runAction(id: string): void {
        if (this.configEditor) {
            const action = this.configEditor.getAction(id)
            action.run().then(
                () => undefined,
                error => logger.error(error)
            )
        } else {
            alert('Wait for editor to load before running action.')
        }
    }
}
