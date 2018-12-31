import EyeIcon from 'mdi-react/EyeIcon'
import InformationOutlineIcon from 'mdi-react/InformationOutlineIcon'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Link } from 'react-router-dom'
import { ConfiguredRegistryExtension } from '../../../../shared/src/extensions/extension'
import extensionSchemaJSON from '../../../../shared/src/schema/extension.schema.json'
import { PageTitle } from '../../components/PageTitle'
import { DynamicallyImportedMonacoSettingsEditor } from '../../settings/DynamicallyImportedMonacoSettingsEditor'
import { eventLogger } from '../../tracking/eventLogger'
import { ExtensionAreaRouteContext } from './ExtensionArea'

export const ExtensionNoManifestAlert: React.FunctionComponent<{
    extension: ConfiguredRegistryExtension
}> = ({ extension }) => (
    <div className="alert alert-info">
        This extension is not yet published.
        {extension.registryExtension && extension.registryExtension.viewerCanAdminister && (
            <>
                <br />
                <Link className="mt-3 btn btn-primary" to={`${extension.registryExtension.url}/-/releases/new`}>
                    Publish first release of extension
                </Link>
            </>
        )}
    </div>
)

interface Props extends ExtensionAreaRouteContext, RouteComponentProps<{}> {
    isLightTheme: boolean
}

interface State {
    viewMode: ViewMode
}

enum ViewMode {
    Rich = 'rich',
    Plain = 'plain',
}

const EXTRA_SCHEMAS = [extensionSchemaJSON]

/** A page that displays an extension's manifest. */
export class RegistryExtensionManifestPage extends React.PureComponent<Props, State> {
    private static STORAGE_KEY = 'RegistryExtensionManifestPage.viewMode'
    private static getViewMode(): ViewMode {
        const v = localStorage.getItem(RegistryExtensionManifestPage.STORAGE_KEY)
        if (v === ViewMode.Rich || v === ViewMode.Plain) {
            return v
        }
        return ViewMode.Rich
    }
    private static setViewMode(value: ViewMode): void {
        localStorage.setItem(RegistryExtensionManifestPage.STORAGE_KEY, value)
    }

    public state: State = { viewMode: RegistryExtensionManifestPage.getViewMode() }

    public componentDidMount(): void {
        eventLogger.logViewEvent('RegistryExtensionManifest')
    }

    public render(): JSX.Element | null {
        return (
            <div className="registry-extension-manifest-page">
                <PageTitle title={`Manifest of ${this.props.extension.id}`} />
                <div className="d-flex align-items-center justify-content-between">
                    <div className="d-flex align-items-center">
                        <h3 className="mb-0 mr-1">Manifest</h3>
                        <InformationOutlineIcon
                            className="icon-inline text-muted"
                            data-tooltip="The published JSON description of how to run or access the extension"
                        />
                    </div>
                    <div>
                        {this.props.extension.manifest && (
                            <button type="button" className="btn btn-secondary" onClick={this.onViewModeButtonClick}>
                                <EyeIcon className="icon-inline" /> Use{' '}
                                {this.state.viewMode === ViewMode.Plain ? ViewMode.Rich : ViewMode.Plain} viewer
                            </button>
                        )}{' '}
                        {this.props.extension.registryExtension &&
                            this.props.extension.registryExtension.viewerCanAdminister && (
                                <Link
                                    className="btn btn-primary"
                                    to={`${this.props.extension.registryExtension.url}/-/releases/new`}
                                >
                                    Publish new release
                                </Link>
                            )}
                    </div>
                </div>
                <div className="mt-2">
                    {this.props.extension.rawManifest === null ? (
                        <ExtensionNoManifestAlert extension={this.props.extension} />
                    ) : this.state.viewMode === ViewMode.Rich ? (
                        <DynamicallyImportedMonacoSettingsEditor
                            id="registry-extension-edit-page__data"
                            value={this.props.extension.rawManifest}
                            height={500}
                            jsonSchemaId="extension.schema.json#"
                            extraSchemas={EXTRA_SCHEMAS}
                            readOnly={true}
                            isLightTheme={this.props.isLightTheme}
                            history={this.props.history}
                        />
                    ) : (
                        // tslint:disable-next-line:jsx-ban-props
                        <pre className="form-control" style={{ minHeight: '30rem' }}>
                            <code>{this.props.extension.rawManifest}</code>
                        </pre>
                    )}
                </div>
            </div>
        )
    }

    private onViewModeButtonClick = () => {
        this.setState(
            prevState => ({ viewMode: prevState.viewMode === ViewMode.Rich ? ViewMode.Plain : ViewMode.Rich }),
            () => RegistryExtensionManifestPage.setViewMode(this.state.viewMode)
        )
    }
}
