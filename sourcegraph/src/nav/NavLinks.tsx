import { ActionsNavItems } from '@sourcegraph/extensions-client-common/lib/app/actions/ActionsNavItems'
import { CommandListPopoverButton } from '@sourcegraph/extensions-client-common/lib/app/CommandList'
import * as H from 'history'
import * as React from 'react'
import { Link } from 'react-router-dom'
import { Subscription } from 'rxjs'
import { ContributableMenu } from 'sourcegraph/module/protocol'
import * as GQL from '../backend/graphqlschema'
import { isDiscussionsEnabled } from '../discussions'
import {
    ConfigurationCascadeProps,
    ExtensionsControllerProps,
    ExtensionsProps,
} from '../extensions/ExtensionsClientCommonContext'
import { KeybindingsProps } from '../keybindings'
import { eventLogger } from '../tracking/eventLogger'
import { showDotComMarketing } from '../util/features'
import { UserNavItem } from './UserNavItem'

interface Props extends ConfigurationCascadeProps, ExtensionsProps, ExtensionsControllerProps, KeybindingsProps {
    location: H.Location
    history: H.History
    authenticatedUser: GQL.IUser | null
    isLightTheme: boolean
    onThemeChange: () => void
    isMainPage?: boolean
}

export class NavLinks extends React.PureComponent<Props> {
    private subscriptions = new Subscription()

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    private onClickInstall = (): void => {
        eventLogger.log('InstallSourcegraphServerCTAClicked', {
            location_on_page: 'Navbar',
        })
    }

    public render(): JSX.Element | null {
        return (
            <ul className="nav-links nav align-items-center pl-2 pr-1">
                {showDotComMarketing && (
                    <li className="nav-item">
                        <a
                            href="https://about.sourcegraph.com"
                            className="nav-link text-truncate"
                            onClick={this.onClickInstall}
                            title="Install self-hosted Sourcegraph to search your own code"
                        >
                            Install <span className="nav-links__widescreen-only">Sourcegraph</span>
                        </a>
                    </li>
                )}
                <ActionsNavItems
                    menu={ContributableMenu.GlobalNav}
                    extensionsController={this.props.extensionsController}
                    extensions={this.props.extensions}
                    location={this.props.location}
                />
                <li className="nav-item">
                    <Link to="/explore" className="nav-link">
                        Explore
                    </Link>
                </li>
                {!this.props.authenticatedUser && (
                    <>
                        <li className="nav-item">
                            <Link to="/extensions" className="nav-link">
                                Extensions
                            </Link>
                        </li>
                        {this.props.location.pathname !== '/sign-in' && (
                            <li className="nav-item mx-1">
                                <Link className="nav-link btn btn-primary" to="/sign-in">
                                    Sign in
                                </Link>
                            </li>
                        )}
                        {showDotComMarketing && (
                            <li className="nav-item">
                                <a href="https://about.sourcegraph.com" className="nav-link">
                                    About
                                </a>
                            </li>
                        )}
                        <li className="nav-item">
                            <Link to="/help" className="nav-link">
                                Help
                            </Link>
                        </li>
                    </>
                )}
                <CommandListPopoverButton
                    menu={ContributableMenu.CommandPalette}
                    extensionsController={this.props.extensionsController}
                    extensions={this.props.extensions}
                    toggleVisibilityKeybinding={this.props.keybindings.commandPalette}
                    location={this.props.location}
                />
                {this.props.authenticatedUser && (
                    <li className="nav-item">
                        <UserNavItem
                            {...this.props}
                            authenticatedUser={this.props.authenticatedUser}
                            showAbout={showDotComMarketing}
                            showDiscussions={isDiscussionsEnabled(this.props.configurationCascade)}
                        />
                    </li>
                )}
            </ul>
        )
    }
}
