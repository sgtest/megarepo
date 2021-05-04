import classNames from 'classnames'
import H from 'history'
import ChevronDownIcon from 'mdi-react/ChevronDownIcon'
import ChevronUpIcon from 'mdi-react/ChevronUpIcon'
import MenuIcon from 'mdi-react/MenuIcon'
import React, { useState } from 'react'
import { LinkProps, NavLink as RouterLink } from 'react-router-dom'

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
    children: React.ReactNode
}

interface NavActionsProps {
    children: React.ReactNode
}

interface NavLinkProps extends NavItemProps, Pick<LinkProps<H.LocationState>, 'to'> {
    external?: boolean
}

export const NavBar = ({ children, logo }: NavBarProps): JSX.Element => (
    <nav aria-label="Main Menu" className={navBarStyles.navbar}>
        <h1 className={navBarStyles.logo}>
            <RouterLink to="/search">{logo}</RouterLink>
        </h1>
        <hr className={navBarStyles.divider} />
        {children}
    </nav>
)

export const NavGroup = ({ children }: NavGroupProps): JSX.Element => {
    const [open, setOpen] = useState(true)

    return (
        <>
            <button
                className={classNames('btn', navBarStyles.menuButton)}
                type="button"
                onClick={() => setOpen(() => !open)}
                aria-label="Sections Navigation"
            >
                <MenuIcon className="icon-inline" />
                {open ? <ChevronDownIcon className="icon-inline" /> : <ChevronUpIcon className="icon-inline" />}
            </button>
            <ul className={classNames(navBarStyles.list, { [navBarStyles.menuClose]: open })}>{children}</ul>
        </>
    )
}

export const NavActions: React.FunctionComponent<NavActionsProps> = ({ children }) => (
    <ul className={navActionStyles.actions}>{children}</ul>
)

export const NavAction: React.FunctionComponent<NavActionsProps> = ({ children }) => (
    <>
        {React.Children.map(children, action => (
            <li className={navActionStyles.action}>{action}</li>
        ))}
    </>
)

export const NavItem: React.FunctionComponent<NavItemProps> = ({ children, icon }) => {
    if (!children) {
        throw new Error('NavItem must be include at least one child')
    }

    return (
        <>
            {React.Children.map(children, child => (
                <li className={navItemStyles.item}>{React.cloneElement(child as React.ReactElement, { icon })}</li>
            ))}
        </>
    )
}

export const NavLink: React.FunctionComponent<NavLinkProps> = ({ icon: Icon, children, to, external }) => {
    const content = (
        <span className={navItemStyles.linkContent}>
            {Icon ? <Icon className={classNames('icon-inline', navItemStyles.icon)} /> : null}
            <span
                className={classNames(navItemStyles.text, {
                    [navItemStyles.iconIncluded]: Icon,
                })}
            >
                {children}
            </span>
        </span>
    )

    if (external) {
        return (
            <a href={to as string} className={navItemStyles.link}>
                {content}
            </a>
        )
    }

    return (
        <RouterLink to={to} className={navItemStyles.link} activeClassName={navItemStyles.active}>
            {content}
        </RouterLink>
    )
}
