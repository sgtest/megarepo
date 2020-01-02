import { parseISO } from 'date-fns'
import differenceInDays from 'date-fns/differenceInDays'
import * as React from 'react'
import { Subscription } from 'rxjs'
import { Markdown } from '../../../shared/src/components/Markdown'
import { isSettingsValid, SettingsCascadeProps } from '../../../shared/src/settings/settings'
import { renderMarkdown } from '../../../shared/src/util/markdown'
import { DismissibleAlert } from '../components/DismissibleAlert'
import { Settings } from '../schema/settings.schema'
import { SiteFlags } from '../site'
import { siteFlags } from '../site/backend'
import { DockerForMacAlert } from '../site/DockerForMacAlert'
import { FreeUsersExceededAlert } from '../site/FreeUsersExceededAlert'
import { LicenseExpirationAlert } from '../site/LicenseExpirationAlert'
import { NeedsRepositoryConfigurationAlert } from '../site/NeedsRepositoryConfigurationAlert'
import { UpdateAvailableAlert } from '../site/UpdateAvailableAlert'
import { GlobalAlert } from './GlobalAlert'
import { Notices } from './Notices'

// This module is not in @types/semver yet. We can't use the top-level semver module because it uses
// dynamic requires, which Webpack complains about.
//
// eslint-disable-next-line @typescript-eslint/ban-ts-ignore
// @ts-ignore
import semverParse from 'semver/functions/parse'

interface Props extends SettingsCascadeProps {
    isSiteAdmin: boolean
}

interface State {
    siteFlags?: SiteFlags
}

/**
 * Fetches and displays relevant global alerts at the top of the page
 */
export class GlobalAlerts extends React.PureComponent<Props, State> {
    public state: State = {}

    private subscriptions = new Subscription()

    public componentDidMount(): void {
        this.subscriptions.add(siteFlags.subscribe(siteFlags => this.setState({ siteFlags })))
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        return (
            <div className="global-alerts e2e-global-alert">
                {this.state.siteFlags && (
                    <>
                        {this.state.siteFlags.needsRepositoryConfiguration && (
                            <NeedsRepositoryConfigurationAlert className="global-alerts__alert" />
                        )}
                        {this.props.isSiteAdmin &&
                            this.state.siteFlags.updateCheck &&
                            !this.state.siteFlags.updateCheck.errorMessage &&
                            this.state.siteFlags.updateCheck.updateVersionAvailable &&
                            ((isSettingsValid<Settings>(this.props.settingsCascade) &&
                                this.props.settingsCascade.final['alerts.showPatchUpdates'] !== false) ||
                                isMinorUpdateAvailable(
                                    this.state.siteFlags.productVersion,
                                    this.state.siteFlags.updateCheck.updateVersionAvailable
                                )) && (
                                <UpdateAvailableAlert
                                    className="global-alerts__alert"
                                    updateVersionAvailable={this.state.siteFlags.updateCheck.updateVersionAvailable}
                                />
                            )}
                        {this.state.siteFlags.freeUsersExceeded && (
                            <FreeUsersExceededAlert
                                noLicenseWarningUserCount={
                                    this.state.siteFlags.productSubscription.noLicenseWarningUserCount
                                }
                                className="global-alerts__alert"
                            />
                        )}
                        {/* Only show if the user has already added repositories; if not yet, the user wouldn't experience any Docker for Mac perf issues anyway. */}
                        {window.context.likelyDockerOnMac && <DockerForMacAlert className="global-alerts__alert" />}
                        {this.state.siteFlags.alerts.map((alert, i) => (
                            <GlobalAlert key={i} alert={alert} className="global-alerts__alert" />
                        ))}
                        {this.state.siteFlags.productSubscription.license &&
                            (() => {
                                const expiresAt = parseISO(this.state.siteFlags.productSubscription.license.expiresAt)
                                return (
                                    differenceInDays(expiresAt, Date.now()) <= 7 && (
                                        <LicenseExpirationAlert
                                            expiresAt={expiresAt}
                                            daysLeft={Math.floor(differenceInDays(expiresAt, Date.now()))}
                                            className="global-alerts__alert"
                                        />
                                    )
                                )
                            })()}
                    </>
                )}
                {isSettingsValid<Settings>(this.props.settingsCascade) &&
                    this.props.settingsCascade.final.motd &&
                    Array.isArray(this.props.settingsCascade.final.motd) &&
                    this.props.settingsCascade.final.motd.map(m => (
                        <DismissibleAlert
                            key={m}
                            partialStorageKey={`motd.${m}`}
                            className="alert alert-info global-alerts__alert"
                        >
                            <Markdown dangerousInnerHTML={renderMarkdown(m)} />
                        </DismissibleAlert>
                    ))}
                <Notices
                    alertClassName="global-alerts__alert"
                    location="top"
                    settingsCascade={this.props.settingsCascade}
                />
            </div>
        )
    }
}

function isMinorUpdateAvailable(currentVersion: string, updateVersion: string): boolean {
    const cv = semverParse(currentVersion, { loose: false })
    const uv = semverParse(updateVersion, { loose: false })
    // If either current or update versions aren't semvers (e.g., a user is on a date-based build version, or "dev"),
    // always return true and allow any alerts to be shown. This has the effect of simply deferring to the response
    // from Sourcegraph.com about whether an update alert is needed.
    if (cv === null || uv === null) {
        return true
    }
    return cv.major !== uv.major || cv.minor !== uv.minor
}
