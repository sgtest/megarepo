import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Link } from 'react-router-dom'
import { Subject, Subscription } from 'rxjs'
import { catchError, concatMap, delay, mergeMap, retryWhen, tap, timeout } from 'rxjs/operators'
import siteSchemaJSON from '../../../schema/site.schema.json'
import * as GQL from '../../../shared/src/graphql/schema'
import { PageTitle } from '../components/PageTitle'
import { DynamicallyImportedMonacoSettingsEditor } from '../settings/DynamicallyImportedMonacoSettingsEditor'
import { refreshSiteFlags } from '../site/backend'
import { eventLogger } from '../tracking/eventLogger'
import { fetchSite, reloadSite, updateSiteConfiguration } from './backend'
import { SiteAdminManagementConsolePassword } from './SiteAdminManagementConsolePassword'

interface Props extends RouteComponentProps<any> {
    isLightTheme: boolean
}

interface State {
    site?: GQL.ISite
    loading: boolean
    error?: Error

    isDirty?: boolean
    saving?: boolean
    restartToApply: boolean
    reloadStartedAt?: number
}

const EXPECTED_RELOAD_WAIT = 7 * 1000 // 7 seconds

const EXTRA_SCHEMAS = [siteSchemaJSON]

/**
 * A page displaying the site configuration.
 */
export class SiteAdminConfigurationPage extends React.Component<Props, State> {
    public state: State = {
        loading: true,
        restartToApply: window.context.needServerRestart,
    }

    private remoteRefreshes = new Subject<void>()
    private remoteUpdates = new Subject<string>()
    private siteReloads = new Subject<void>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        eventLogger.logViewEvent('SiteAdminConfiguration')

        this.subscriptions.add(
            this.remoteRefreshes.pipe(mergeMap(() => fetchSite())).subscribe(
                site =>
                    this.setState({
                        site,
                        error: undefined,
                        loading: false,
                    }),
                error => this.setState({ error, loading: false })
            )
        )
        this.remoteRefreshes.next()

        this.subscriptions.add(
            this.remoteUpdates
                .pipe(
                    tap(() => this.setState({ saving: true, error: undefined })),
                    concatMap(newContents => {
                        const lastConfiguration = this.state.site && this.state.site.configuration
                        const lastConfigurationID = (lastConfiguration && lastConfiguration.id) || 0

                        return updateSiteConfiguration(lastConfigurationID, newContents).pipe(
                            catchError(error => {
                                console.error(error)
                                this.setState({ saving: false, error })
                                return []
                            })
                        )
                    }),
                    tap(restartToApply => {
                        if (restartToApply) {
                            window.context.needServerRestart = restartToApply
                        } else {
                            // Refresh site flags so that global site alerts
                            // reflect the latest configuration.
                            refreshSiteFlags().subscribe(undefined, err => console.error(err))
                        }
                        this.setState({ restartToApply })
                        this.remoteRefreshes.next()
                    })
                )
                .subscribe(() => this.setState({ saving: false }), error => this.setState({ saving: false, error }))
        )

        this.subscriptions.add(
            this.siteReloads
                .pipe(
                    tap(() => this.setState({ reloadStartedAt: Date.now(), error: undefined })),
                    mergeMap(reloadSite),
                    delay(2000),
                    mergeMap(() =>
                        // wait for server to restart
                        fetchSite().pipe(
                            retryWhen(x =>
                                x.pipe(
                                    tap(() => this.forceUpdate()),
                                    delay(500)
                                )
                            ),
                            timeout(10000)
                        )
                    ),
                    tap(() => this.remoteRefreshes.next())
                )
                .subscribe(
                    () => {
                        this.setState({ reloadStartedAt: undefined })
                        window.location.reload() // brute force way to reload view state
                    },
                    error => this.setState({ reloadStartedAt: undefined, error })
                )
        )
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        const alerts: JSX.Element[] = []
        if (this.state.error) {
            alerts.push(
                <div key="error" className="alert alert-danger site-admin-configuration-page__alert">
                    <p>Error: {this.state.error.message}</p>
                </div>
            )
        }
        if (this.state.reloadStartedAt) {
            alerts.push(
                <div key="error" className="alert alert-primary site-admin-configuration-page__alert">
                    <p>
                        <LoadingSpinner className="icon-inline" /> Waiting for site to reload...
                    </p>
                    {Date.now() - this.state.reloadStartedAt > EXPECTED_RELOAD_WAIT && (
                        <p>
                            <small>It's taking longer than expected. Check the server logs for error messages.</small>
                        </p>
                    )}
                </div>
            )
        }
        if (this.state.restartToApply) {
            alerts.push(
                <div
                    key="remote-dirty"
                    className="alert alert-warning site-admin-configuration-page__alert site-admin-configuration-page__alert-flex"
                >
                    Server restart is required for the configuration to take effect.
                    <button className="btn btn-primary btn-sm" onClick={this.reloadSite}>
                        Restart server
                    </button>
                </div>
            )
        }
        if (
            this.state.site &&
            this.state.site.configuration &&
            this.state.site.configuration.validationMessages &&
            this.state.site.configuration.validationMessages.length > 0
        ) {
            alerts.push(
                <div key="validation-messages" className="alert alert-danger site-admin-configuration-page__alert">
                    <p>The server reported issues in the last-saved config:</p>
                    <ul>
                        {this.state.site.configuration.validationMessages.map((e, i) => (
                            <li key={i} className="site-admin-configuration-page__alert-item">
                                {e}
                            </li>
                        ))}
                    </ul>
                </div>
            )
        }

        // Avoid user confusion with values.yaml properties mixed in with site config properties.
        const contents =
            this.state.site && this.state.site.configuration && this.state.site.configuration.effectiveContents
        const legacyKubernetesConfigProps = [
            'alertmanagerConfig',
            'alertmanagerURL',
            'authProxyIP',
            'authProxyPassword',
            'deploymentOverrides',
            'gitoliteIP',
            'gitserverCount',
            'gitserverDiskSize',
            'gitserverSSH',
            'httpNodePort',
            'httpsNodePort',
            'indexedSearchDiskSize',
            'langGo',
            'langJava',
            'langJavaScript',
            'langPHP',
            'langPython',
            'langSwift',
            'langTypeScript',
            'nodeSSDPath',
            'phabricatorIP',
            'prometheus',
            'pyPIIP',
            'rbac',
            'storageClass',
            'useAlertManager',
        ].filter(prop => contents && contents.includes(`"${prop}"`))
        if (legacyKubernetesConfigProps.length > 0) {
            alerts.push(
                <div
                    key="legacy-cluster-props-present"
                    className="alert alert-info site-admin-configuration-page__alert"
                >
                    The configuration contains properties that are valid only in the
                    <code>values.yaml</code> config file used for Kubernetes cluster deployments of Sourcegraph:{' '}
                    <code>{legacyKubernetesConfigProps.join(' ')}</code>. You can disregard the validation warnings for
                    these properties reported by the configuration editor.
                </div>
            )
        }

        const isReloading = typeof this.state.reloadStartedAt === 'number'

        return (
            <div className="site-admin-configuration-page">
                <PageTitle title="Configuration - Admin" />
				<div className="d-flex justify-content-between align-items-center mt-3 mb-1">
					<h2 className="mb-0">Site configuration</h2>
				</div>
				<p>
                    View and edit the Sourcegraph site configuration. See{' '}
                    <Link to="/help/admin/site_config">documentation</Link> for more information.
                </p>
                <div className="mb-3">
                    <SiteAdminManagementConsolePassword />
                </div>
                <div className="site-admin-configuration-page__alerts">{alerts}</div>
                {this.state.loading && <LoadingSpinner className="icon-inline" />}
                {this.state.site && this.state.site.configuration && (
                    <div>
                        <DynamicallyImportedMonacoSettingsEditor
                            value={contents || ''}
                            jsonSchemaId="site.schema.json#"
                            extraSchemas={EXTRA_SCHEMAS}
                            onDirtyChange={this.onDirtyChange}
                            canEdit={true}
                            saving={this.state.saving}
                            loading={isReloading || this.state.saving}
                            height={600}
                            isLightTheme={this.props.isLightTheme}
                            onSave={this.onSave}
                            history={this.props.history}
                        />
                        <p className="form-text text-muted">
                            <small>
                                Use Ctrl+Space for completion, and hover over JSON properties for documentation. For
                                more information, see the <Link to="/help/admin/site_config/all">documentation</Link>.
                            </small>
                        </p>
                    </div>
                )}
            </div>
        )
    }

    private onDirtyChange = (isDirty: boolean) => this.setState({ isDirty })

    private onSave = (value: string) => {
        eventLogger.log('SiteConfigurationSaved')
        this.remoteUpdates.next(value)
    }

    private reloadSite = () => {
        eventLogger.log('SiteReloaded')
        this.siteReloads.next()
    }
}
