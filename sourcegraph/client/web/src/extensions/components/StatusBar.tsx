import classNames from 'classnames'
import * as H from 'history'
import MenuLeftIcon from 'mdi-react/MenuLeftIcon'
import MenuRightIcon from 'mdi-react/MenuRightIcon'
import React, { useCallback, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { Observable, timer } from 'rxjs'
import { filter, first, mapTo, switchMap } from 'rxjs/operators'

import { urlForClientCommandOpen } from '@sourcegraph/shared/src/actions/ActionItem'
import { StatusBarItemWithKey } from '@sourcegraph/shared/src/api/extension/api/codeEditor'
import { haveInitialExtensionsLoaded } from '@sourcegraph/shared/src/api/features'
import { ButtonLink } from '@sourcegraph/shared/src/components/LinkOrButton'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'

import { ErrorBoundary } from '../../components/ErrorBoundary'
import { useCarousel } from '../../components/useCarousel'

interface StatusBarProps extends ExtensionsControllerProps<'extHostAPI' | 'executeCommand'> {
    getStatusBarItems: () => Observable<StatusBarItemWithKey[] | 'loading'>
    className?: string
    /**
     * Used to determine when to restart timer to show "Install extensions"
     * message when there are no status bar items. Only necessary when status bar
     * persists beteween files (e.g. for `<Blob>`).
     */
    uri?: string

    location: H.Location
}

export const StatusBar: React.FunctionComponent<StatusBarProps> = ({
    getStatusBarItems,
    className,
    extensionsController,
    uri,
    location,
}) => {
    const statusBarItems = useObservable(useMemo(() => getStatusBarItems(), [getStatusBarItems]))

    const haveExtensionsLoaded = useObservable(
        useMemo(() => haveInitialExtensionsLoaded(extensionsController.extHostAPI), [extensionsController])
    )

    // Wait a generous amount of time on top of initial extension loading
    // before showing "Install extensions" message to be forgiving of extensions
    // that make slow network requests and forget to indicate loading state.
    const hasEnoughTimePassed = useObservable(
        useMemo(
            () =>
                haveInitialExtensionsLoaded(extensionsController.extHostAPI).pipe(
                    filter(haveLoaded => haveLoaded),
                    first(),
                    switchMap(() => timer(2000).pipe(mapTo(true)))
                ),
            // We want to recreate the observable on uri change, so keep
            // the unnecessary dependency.
            // eslint-disable-next-line react-hooks/exhaustive-deps
            [uri, extensionsController]
        )
    )

    const {
        carouselReference,
        canScrollNegative,
        canScrollPositive,
        onNegativeClicked,
        onPositiveClicked,
    } = useCarousel({ direction: 'leftToRight' })

    return (
        <div className={classNames('status-bar w-100 border-top d-flex', className)}>
            <ErrorBoundary
                location={location}
                // To be clear to users that this isn't an error reported by extensions
                // about e.g. the code they're viewing.
                // eslint-disable-next-line react/jsx-no-bind
                render={error => (
                    <div className="status-bar__item ml-2">
                        <small className="text-muted">Status bar component error: {error.message}</small>
                    </div>
                )}
            >
                {canScrollNegative && (
                    <button
                        type="button"
                        className="btn btn-link status-bar__scroll border-0"
                        onClick={onNegativeClicked}
                    >
                        <MenuLeftIcon className="icon-inline" />
                    </button>
                )}
                <div className="status-bar__items d-flex px-2" ref={carouselReference}>
                    {!!statusBarItems && statusBarItems !== 'loading' && statusBarItems.length > 0
                        ? statusBarItems.map(statusBarItem => (
                              <StatusBarItem
                                  key={statusBarItem.key}
                                  statusBarItem={statusBarItem}
                                  extensionsController={extensionsController}
                                  location={location}
                              />
                          ))
                        : haveExtensionsLoaded &&
                          hasEnoughTimePassed && (
                              <div className="status-bar__item ml-2">
                                  <small className="text-muted">
                                      No information from extensions available.{' '}
                                      <Link to="/extensions">
                                          Find extensions in the Sourcegraph extension registry
                                      </Link>
                                  </small>
                              </div>
                          )}
                </div>
                {canScrollPositive && (
                    <button
                        type="button"
                        className="btn btn-link status-bar__scroll border-0"
                        onClick={onPositiveClicked}
                    >
                        <MenuRightIcon className="icon-inline" />
                    </button>
                )}
            </ErrorBoundary>
        </div>
    )
}

const StatusBarItem: React.FunctionComponent<
    {
        statusBarItem: StatusBarItemWithKey
        className?: string
        component?: JSX.Element
        location: H.Location
    } & ExtensionsControllerProps<'extHostAPI' | 'executeCommand'>
> = ({ statusBarItem, className = 'status-bar', component, extensionsController, location }) => {
    const [commandState, setCommandState] = useState<'loading' | null>(null)

    const command = useMemo(() => statusBarItem.command, [statusBarItem.command])

    const to = useMemo(
        () => command && urlForClientCommandOpen({ command: command.id, commandArguments: command.args }, location),
        [command, location]
    )

    const handleCommand = useCallback(() => {
        // Do not execute the command if `to` is defined.
        // The <ButtonLink>'s default event handler will do what we want (which is to open a URL).
        if (commandState !== 'loading' && command && !to) {
            setCommandState('loading')
            extensionsController
                .executeCommand({ command: command.id, args: command.args })
                .then(() => {
                    setCommandState(null)
                })
                .catch(() => {
                    // noop, errors will be displayed as notifications
                    setCommandState(null)
                })
        }
    }, [commandState, extensionsController, command, to])

    const noop = !command

    return (
        <ButtonLink
            className={classNames(
                `${className}__item h-100 d-flex align-items-center px-1 text-decoration-none`,
                statusBarItem.tooltip && `${className}__item--tooltipped`,
                noop && `${className}__item--noop`
            )}
            data-tooltip={statusBarItem.tooltip}
            onSelect={handleCommand}
            tabIndex={noop ? -1 : 0}
            to={to}
            disabled={commandState === 'loading'}
        >
            {component || (
                <small className={classNames(`${className}__text`, commandState === 'loading' && 'text-muted')}>
                    {statusBarItem.text}
                </small>
            )}
        </ButtonLink>
    )
}
