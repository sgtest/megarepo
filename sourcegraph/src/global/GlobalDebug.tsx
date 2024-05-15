import { ExtensionStatusPopover } from '@sourcegraph/extensions-client-common/lib/app/ExtensionStatus'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as H from 'history'
import MenuDownIcon from 'mdi-react/MenuDownIcon'
import * as React from 'react'
import * as GQL from '../backend/graphqlschema'
import { ExtensionsEnvironmentProps } from '../extensions/environment/ExtensionsEnvironment'
import { ExtensionsControllerProps } from '../extensions/ExtensionsClientCommonContext'

interface Props extends ExtensionsEnvironmentProps, ExtensionsControllerProps {
    user: GQL.IUser | null
    location: H.Location
}

const SHOW_DEBUG = localStorage.getItem('debug') !== null

/**
 * A global debug toolbar shown in the bottom right of the window.
 */
export const GlobalDebug: React.SFC<Props> = props =>
    SHOW_DEBUG ? (
        <div className="global-debug navbar navbar-expand">
            <ul className="navbar-nav align-items-center">
                <li className="nav-item">
                    <ExtensionStatusPopover
                        location={props.location}
                        loaderIcon={LoadingSpinner as React.ComponentType<{ className: string; onClick?: () => void }>}
                        caretIcon={MenuDownIcon as React.ComponentType<{ className: string; onClick?: () => void }>}
                        extensionsController={props.extensionsController}
                    />
                </li>
            </ul>
        </div>
    ) : null
