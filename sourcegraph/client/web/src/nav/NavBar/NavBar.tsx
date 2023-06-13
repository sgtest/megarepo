import React, { forwardRef, useContext } from 'react'

import { mdiMenu } from '@mdi/js'
import classNames from 'classnames'
import { LinkProps, NavLink as RouterNavLink } from 'react-router-dom'

import {
    Link,
    Icon,
    H1,
    ForwardReferenceComponent,
    VIEWPORT_SM,
    Menu,
    MenuList,
    MenuButton,
    MenuLink,
    useMatchMedia,
} from '@sourcegraph/wildcard'

import { PageRoutes } from '../../routes.constants'

import navActionStyles from './NavAction.module.scss'
import navBarStyles from './NavBar.module.scss'
import navItemStyles from './NavItem.module.scss'

interface NavBarProps {
    children: React.ReactNode
    logo: React.ReactNode
}

interface NavGroupProps {
    children: React.ReactNode
}

interface NavItemProps {
    icon?: React.ComponentType<{ className?: string }>
    className?: string
    children: React.ReactNode
}

interface NavActionsProps {
    children: React.ReactNode
    className?: string
}

export interface NavLinkProps extends NavItemProps, Pick<LinkProps, 'to'> {
    external?: boolean
    className?: string
    variant?: 'compact'
}

export const NavBar = forwardRef(function NavBar({ children, logo }, reference): JSX.Element {
    return (
        <nav aria-label="Main" className={navBarStyles.navbar} ref={reference}>
            <H1 className={navBarStyles.logo}>
                <RouterNavLink className="d-flex align-items-center" to={PageRoutes.Search}>
                    {logo}
                </RouterNavLink>
            </H1>
            <hr className={navBarStyles.divider} aria-hidden={true} />
            {children}
        </nav>
    )
}) as ForwardReferenceComponent<'nav', NavBarProps>

export const MobileNavGroupContext = React.createContext(false)

export const NavGroup = ({ children }: NavGroupProps): JSX.Element => {
    const isMobileSize = useMatchMedia(`(max-width: ${VIEWPORT_SM}px)`)

    return (
        <MobileNavGroupContext.Provider value={isMobileSize}>
            {isMobileSize ? (
                <Menu>
                    <MenuButton aria-label="Sections Navigation">
                        <Icon aria-hidden={true} svgPath={mdiMenu} />
                    </MenuButton>
                    <MenuList>{children}</MenuList>
                </Menu>
            ) : (
                <div className={navBarStyles.menu}>
                    <ul className={navBarStyles.list}>{children}</ul>
                </div>
            )}
        </MobileNavGroupContext.Provider>
    )
}

export const NavActions: React.FunctionComponent<React.PropsWithChildren<NavActionsProps>> = ({ children }) => (
    <ul className={navActionStyles.actions}>{children}</ul>
)

export const NavAction: React.FunctionComponent<React.PropsWithChildren<NavActionsProps>> = ({
    children,
    className,
}) => (
    <>
        {React.Children.map(children, action => (
            <li className={classNames(navActionStyles.action, className)}>{action}</li>
        ))}
    </>
)

export const NavItem: React.FunctionComponent<React.PropsWithChildren<NavItemProps>> = ({
    children,
    className,
    icon,
}) => {
    const mobileNav = useContext(MobileNavGroupContext)

    if (mobileNav) {
        return <>{React.Children.map(children, child => React.cloneElement(child as React.ReactElement, { icon }))}</>
    }

    if (!children) {
        throw new Error('NavItem must be include at least one child')
    }

    return (
        <>
            {React.Children.map(children, child => (
                <li className={classNames(navItemStyles.item, className)}>
                    {React.cloneElement(child as React.ReactElement, { icon })}
                </li>
            ))}
        </>
    )
}

export const NavLink: React.FunctionComponent<React.PropsWithChildren<NavLinkProps>> = ({
    icon: LinkIcon,
    children,
    to,
    external,
    variant,
    className,
}) => {
    const mobileNav = useContext(MobileNavGroupContext)

    if (mobileNav) {
        const content = (
            <>
                {LinkIcon ? <Icon className="mr-2" as={LinkIcon} aria-hidden={true} /> : null}
                {children}
            </>
        )
        return (
            <MenuLink
                as={Link}
                to={to as string}
                rel={external ? 'noreferrer noopener' : undefined}
                target={external ? '_blank' : undefined}
                className={className}
            >
                {content}
            </MenuLink>
        )
    }

    const content = (
        <span className={classNames(navItemStyles.linkContent, className)}>
            {LinkIcon ? <Icon className={navItemStyles.icon} as={LinkIcon} aria-hidden={true} /> : null}
            <span
                className={classNames(navItemStyles.text, {
                    [navItemStyles.iconIncluded]: Icon,
                    [navItemStyles.isCompact]: variant === 'compact',
                })}
            >
                {children}
            </span>
        </span>
    )

    if (external) {
        return (
            <Link to={to as string} rel="noreferrer noopener" target="_blank" className={navItemStyles.link}>
                {content}
            </Link>
        )
    }

    return (
        <RouterNavLink
            to={to}
            className={({ isActive }) => classNames(navItemStyles.link, isActive && navItemStyles.active)}
        >
            {content}
        </RouterNavLink>
    )
}
