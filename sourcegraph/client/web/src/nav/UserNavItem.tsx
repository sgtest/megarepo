import { useCallback, useMemo, ChangeEventHandler, FC } from 'react'

import { mdiChevronDown, mdiChevronUp, mdiCogOutline, mdiOpenInNew } from '@mdi/js'
import classNames from 'classnames'

import { Toggle } from '@sourcegraph/branded/src/components/Toggle'
import { UserAvatar } from '@sourcegraph/shared/src/components/UserAvatar'
import { useKeyboardShortcut } from '@sourcegraph/shared/src/keyboardShortcuts/useKeyboardShortcut'
import { Shortcut } from '@sourcegraph/shared/src/react-shortcuts'
import { useExperimentalFeatures } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { useTheme, ThemeSetting } from '@sourcegraph/shared/src/theme'
import {
    Menu,
    MenuButton,
    MenuDivider,
    MenuHeader,
    MenuItem,
    MenuLink,
    MenuList,
    Link,
    Position,
    AnchorLink,
    Select,
    Icon,
    ProductStatusBadge,
} from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { useFeatureFlag } from '../featureFlags/useFeatureFlag'
import { useExperimentalQueryInput } from '../search/useExperimentalSearchInput'

import { AppUserConnectDotComAccount } from './AppUserConnectDotComAccount'

import styles from './UserNavItem.module.scss'

const MAX_VISIBLE_ORGS = 5

type MinimalAuthenticatedUser = Pick<
    AuthenticatedUser,
    'username' | 'avatarURL' | 'settingsURL' | 'organizations' | 'siteAdmin' | 'session' | 'displayName'
>

export interface UserNavItemProps extends TelemetryProps {
    authenticatedUser: MinimalAuthenticatedUser
    isSourcegraphDotCom: boolean
    isSourcegraphApp: boolean
    menuButtonRef?: React.Ref<HTMLButtonElement>
    showFeedbackModal: () => void
    showKeyboardShortcutsHelp: () => void
}

/**
 * Displays the user's avatar and/or username in the navbar and exposes a dropdown menu with more options for
 * authenticated viewers.
 */
export const UserNavItem: FC<UserNavItemProps> = props => {
    const {
        authenticatedUser,
        isSourcegraphDotCom,
        isSourcegraphApp,
        menuButtonRef,
        showFeedbackModal,
        showKeyboardShortcutsHelp,
        telemetryService,
    } = props

    const { themeSetting, setThemeSetting } = useTheme()
    const keyboardShortcutSwitchTheme = useKeyboardShortcut('switchTheme')
    const [enableTeams] = useFeatureFlag('search-ownership')

    const supportsSystemTheme = useMemo(
        () => Boolean(window.matchMedia?.('not all and (prefers-color-scheme), (prefers-color-scheme)').matches),
        []
    )

    const onThemeChange: ChangeEventHandler<HTMLSelectElement> = useCallback(
        event => {
            setThemeSetting(event.target.value as ThemeSetting)
        },
        [setThemeSetting]
    )

    const onThemeCycle = useCallback((): void => {
        setThemeSetting(themeSetting === ThemeSetting.Dark ? ThemeSetting.Light : ThemeSetting.Dark)
    }, [setThemeSetting, themeSetting])

    const organizations = authenticatedUser.organizations.nodes
    const searchQueryInputFeature = useExperimentalFeatures(features => features.searchQueryInput)
    const [experimentalQueryInputEnabled, setExperimentalQueryInputEnabled] = useExperimentalQueryInput()

    const onExperimentalQueryInputChange = useCallback(
        (enabled: boolean) => {
            telemetryService.log(`SearchInputToggle${enabled ? 'On' : 'Off'}`)
            setExperimentalQueryInputEnabled(enabled)
        },
        [telemetryService, setExperimentalQueryInputEnabled]
    )

    return (
        <>
            {keyboardShortcutSwitchTheme?.keybindings.map((keybinding, index) => (
                // `Shortcut` doesn't update its states when `onMatch` changes
                // so we put `themePreference` in `key` binding to make it
                <Shortcut key={`${themeSetting}-${index}`} {...keybinding} onMatch={onThemeCycle} />
            ))}
            <Menu>
                {({ isExpanded }) => (
                    <>
                        <MenuButton
                            ref={menuButtonRef}
                            variant="link"
                            data-testid="user-nav-item-toggle"
                            className={classNames('d-flex align-items-center text-decoration-none', styles.menuButton)}
                            aria-label={`${isExpanded ? 'Close' : 'Open'} user profile menu`}
                        >
                            <div className="position-relative">
                                <div className="align-items-center d-flex">
                                    {isSourcegraphApp ? (
                                        <Icon svgPath={mdiCogOutline} aria-hidden={true} />
                                    ) : (
                                        <UserAvatar user={authenticatedUser} className={styles.avatar} />
                                    )}
                                    <Icon svgPath={isExpanded ? mdiChevronUp : mdiChevronDown} aria-hidden={true} />
                                </div>
                            </div>
                        </MenuButton>

                        <MenuList
                            position={Position.bottomEnd}
                            className={styles.dropdownMenu}
                            aria-label="User. Open menu"
                        >
                            {!isSourcegraphApp ? (
                                <>
                                    <MenuHeader className={styles.dropdownHeader}>
                                        Signed in as <strong>@{authenticatedUser.username}</strong>
                                    </MenuHeader>
                                    <MenuDivider className={styles.dropdownDivider} />
                                </>
                            ) : null}
                            <MenuLink as={Link} to={authenticatedUser.settingsURL!}>
                                Settings
                            </MenuLink>
                            <MenuLink as={Link} to={`/users/${props.authenticatedUser.username}/searches`}>
                                Saved searches
                            </MenuLink>
                            {isSourcegraphApp && (
                                <MenuLink as={Link} to="/site-admin/repositories">
                                    Repositories
                                </MenuLink>
                            )}
                            {isSourcegraphApp && <AppUserConnectDotComAccount />}
                            {enableTeams && !isSourcegraphDotCom && (
                                <MenuLink as={Link} to="/teams">
                                    Teams
                                </MenuLink>
                            )}
                            <MenuDivider />
                            <div className="px-2 py-1">
                                <div className="d-flex align-items-center">
                                    <div className="mr-2">Theme</div>
                                    <Select
                                        aria-label=""
                                        isCustomStyle={true}
                                        selectSize="sm"
                                        data-testid="theme-toggle"
                                        onChange={onThemeChange}
                                        value={themeSetting}
                                        className="mb-0 flex-1"
                                    >
                                        <option value={ThemeSetting.Light}>Light</option>
                                        <option value={ThemeSetting.Dark}>Dark</option>
                                        <option value={ThemeSetting.System}>System</option>
                                    </Select>
                                </div>
                                {themeSetting === ThemeSetting.System && !supportsSystemTheme && (
                                    <div className="text-wrap">
                                        <small>
                                            <AnchorLink
                                                to="https://caniuse.com/#feat=prefers-color-scheme"
                                                className="text-warning"
                                                target="_blank"
                                                rel="noopener noreferrer"
                                            >
                                                Your browser does not support the system theme.
                                            </AnchorLink>
                                        </small>
                                    </div>
                                )}
                            </div>
                            {searchQueryInputFeature === 'experimental' && (
                                <div className="px-2 py-1">
                                    <div className="d-flex align-items-center justify-content-between">
                                        <div className="mr-2">
                                            New search input <ProductStatusBadge status="beta" className="ml-1" />
                                        </div>
                                        <Toggle
                                            value={experimentalQueryInputEnabled}
                                            onToggle={onExperimentalQueryInputChange}
                                        />
                                    </div>
                                </div>
                            )}

                            {organizations.length > 0 && (
                                <>
                                    <MenuDivider className={styles.dropdownDivider} />
                                    <MenuHeader className={styles.dropdownHeader}>Your organizations</MenuHeader>
                                    {organizations.slice(0, MAX_VISIBLE_ORGS).map(org => (
                                        <MenuLink as={Link} key={org.id} to={org.settingsURL || org.url}>
                                            {org.displayName || org.name}
                                        </MenuLink>
                                    ))}
                                    {organizations.length > MAX_VISIBLE_ORGS && (
                                        <MenuLink as={Link} to={authenticatedUser.settingsURL!}>
                                            Show all organizations
                                        </MenuLink>
                                    )}
                                </>
                            )}
                            <MenuDivider className={styles.dropdownDivider} />
                            {authenticatedUser.siteAdmin && !isSourcegraphApp && (
                                <MenuLink as={Link} to="/site-admin">
                                    Site admin
                                </MenuLink>
                            )}
                            <MenuLink as={Link} to="/help" target="_blank" rel="noopener">
                                {isSourcegraphApp ? 'Documentation' : 'Help'}{' '}
                                <Icon aria-hidden={true} svgPath={mdiOpenInNew} />
                            </MenuLink>

                            {isSourcegraphApp ? (
                                <MenuLink as={AnchorLink} to="/user/settings/product-research">
                                    Feedback
                                </MenuLink>
                            ) : (
                                <MenuItem onSelect={showFeedbackModal}>Feedback</MenuItem>
                            )}

                            <MenuItem onSelect={showKeyboardShortcutsHelp}>Keyboard shortcuts</MenuItem>

                            {authenticatedUser.session?.canSignOut && !isSourcegraphApp && (
                                <MenuLink as={AnchorLink} to="/-/sign-out">
                                    Sign out
                                </MenuLink>
                            )}
                            {isSourcegraphDotCom && <MenuDivider className={styles.dropdownDivider} />}
                            {isSourcegraphDotCom && (
                                <MenuLink
                                    as={AnchorLink}
                                    to="https://about.sourcegraph.com"
                                    target="_blank"
                                    rel="noopener"
                                >
                                    About Sourcegraph <Icon aria-hidden={true} svgPath={mdiOpenInNew} />
                                </MenuLink>
                            )}
                        </MenuList>
                    </>
                )}
            </Menu>
        </>
    )
}
