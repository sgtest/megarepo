import ChevronRightIcon from 'mdi-react/ChevronRightIcon'
import * as React from 'react'
import { Link, NavLink, RouteComponentProps } from 'react-router-dom'
import { isExtensionEnabled } from '../../../../shared/src/extensions/extension'
import { ExtensionManifest } from '../../../../shared/src/schema/extension.schema'
import { isErrorLike } from '../../../../shared/src/util/errors'
import { NavItemWithIconDescriptor } from '../../util/contributions'
import { ExtensionToggle } from '../ExtensionToggle'
import { isExtensionAdded } from './extension'
import { ExtensionAreaRouteContext } from './ExtensionArea'
import { ExtensionConfigurationState } from './ExtensionConfigurationState'
import { WorkInProgressBadge } from './WorkInProgressBadge'

interface ExtensionAreaHeaderProps extends ExtensionAreaRouteContext, RouteComponentProps<{}> {
    navItems: ReadonlyArray<ExtensionAreaHeaderNavItem>
}

export type ExtensionAreaHeaderContext = Pick<ExtensionAreaHeaderProps, 'extension'>

export interface ExtensionAreaHeaderNavItem extends NavItemWithIconDescriptor<ExtensionAreaHeaderContext> {}

/**
 * Header for the extension area.
 */
export const ExtensionAreaHeader: React.FunctionComponent<ExtensionAreaHeaderProps> = (
    props: ExtensionAreaHeaderProps
) => {
    const manifest: ExtensionManifest | undefined =
        props.extension.manifest && !isErrorLike(props.extension.manifest) ? props.extension.manifest : undefined
    let iconURL: URL | undefined
    try {
        if (manifest && manifest.icon) {
            iconURL = new URL(manifest.icon)
        }
    } catch (e) {
        // noop
    }

    const isWorkInProgress = props.extension.registryExtension && props.extension.registryExtension.isWorkInProgress

    return (
        <div className="extension-area-header border-bottom simple-area-header">
            <div className="container">
                {props.extension && (
                    <>
                        <div className="mb-3">
                            <div className="d-flex align-items-start">
                                {manifest &&
                                    manifest.icon &&
                                    iconURL &&
                                    iconURL.protocol === 'data:' &&
                                    /^data:image\/png(;base64)?,/.test(manifest.icon) && (
                                        <img className="extension-area-header__icon mr-2" src={manifest.icon} />
                                    )}
                                <div>
                                    <div className="d-flex align-items-center">
                                        <h2 className="mb-0">
                                            <Link to="/extensions" className="extensions-nav-link">
                                                Extensions
                                            </Link>{' '}
                                            <ChevronRightIcon className="icon-inline extension-area-header__icon-chevron" />{' '}
                                            {(manifest && manifest.title) || props.extension.id}
                                        </h2>
                                    </div>
                                    {manifest &&
                                        manifest.title && <div className="text-muted">{props.extension.id}</div>}
                                    {manifest &&
                                        (manifest.description || isWorkInProgress) && (
                                            <p className="mt-1 mb-0">
                                                {isWorkInProgress && (
                                                    <WorkInProgressBadge
                                                        viewerCanAdminister={
                                                            !!props.extension.registryExtension &&
                                                            props.extension.registryExtension.viewerCanAdminister
                                                        }
                                                    />
                                                )}
                                                {manifest.description}
                                            </p>
                                        )}
                                </div>
                            </div>
                        </div>
                        <div className="d-flex align-items-center mt-3 mb-2">
                            {props.authenticatedUser && (
                                <div className="mr-2">
                                    <ExtensionToggle
                                        extension={props.extension}
                                        settingsCascade={props.settingsCascade}
                                        onUpdate={props.onDidUpdateExtension}
                                        addClassName="btn-primary"
                                        platformContext={props.platformContext}
                                    />
                                </div>
                            )}
                            <ExtensionConfigurationState
                                className="mr-2"
                                isAdded={isExtensionAdded(props.settingsCascade.final, props.extension.id)}
                                isEnabled={isExtensionEnabled(props.settingsCascade.final, props.extension.id)}
                            />
                            {!props.authenticatedUser && (
                                <div className="d-flex align-items-center">
                                    <Link to="/sign-in" className="btn btn-primary mr-2">
                                        Sign in to{' '}
                                        {isExtensionEnabled(props.settingsCascade.final, props.extension.id)
                                            ? 'configure'
                                            : 'enable'}
                                    </Link>
                                    <small className="text-muted">
                                        An account is required to{' '}
                                        {isExtensionEnabled(props.settingsCascade.final, props.extension.id)
                                            ? ''
                                            : 'enable and'}{' '}
                                        configure extensions.
                                    </small>
                                </div>
                            )}
                        </div>
                        <div className="area-header__nav mt-3">
                            <div className="area-header__nav-links">
                                {props.navItems.map(
                                    ({ to, label, exact, icon: Icon, condition = () => true }) =>
                                        condition(props) && (
                                            <NavLink
                                                key={label}
                                                to={props.url + to}
                                                className="btn area-header__nav-link"
                                                activeClassName="area-header__nav-link--active"
                                                exact={exact}
                                            >
                                                {Icon && <Icon className="icon-inline" />} {label}
                                            </NavLink>
                                        )
                                )}
                            </div>
                        </div>
                    </>
                )}
            </div>
        </div>
    )
}
