import * as H from 'history'
import * as React from 'react'
import { Controller as ClientController } from '../../../../../shared/src/extensions/controller'
import { ExtensionStatusPopover } from '../../../../../shared/src/extensions/ExtensionStatus'
import { PlatformContextProps } from '../../../../../shared/src/platform/context'
import { sourcegraphUrl } from '../util/context'
import { ShortcutProvider } from './ShortcutProvider'

interface Props extends PlatformContextProps {
    location: H.Location
    extensionsController: ClientController
}

const SHOW_DEBUG = localStorage.getItem('debug') !== null

const ExtensionLink: React.FunctionComponent<{ id: string }> = props => {
    const extensionURL = new URL(sourcegraphUrl)
    extensionURL.pathname = `extensions/${props.id}`
    return <a href={extensionURL.href}>{props.id}</a>
}

/**
 * A global debug toolbar shown in the bottom right of the window.
 */
export const GlobalDebug: React.FunctionComponent<Props> = props =>
    SHOW_DEBUG ? (
        <div className="global-debug navbar navbar-expand">
            <ul className="navbar-nav align-items-center">
                <li className="nav-item">
                    <ShortcutProvider>
                        <ExtensionStatusPopover
                            location={props.location}
                            extensionsController={props.extensionsController}
                            link={ExtensionLink}
                            platformContext={props.platformContext}
                        />
                    </ShortcutProvider>
                </li>
            </ul>
        </div>
    ) : null
