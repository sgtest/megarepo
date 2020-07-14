import * as H from 'history'
import React, { useEffect, useMemo } from 'react'
import {
    PatternTypeProps,
    InteractiveSearchProps,
    CaseSensitivityProps,
    SmartSearchFieldProps,
    CopyQueryButtonProps,
    RepogroupHomepageProps,
} from '..'
import { ActivationProps } from '../../../../shared/src/components/activation/Activation'
import * as GQL from '../../../../shared/src/graphql/schema'
import { SettingsCascadeProps } from '../../../../shared/src/settings/settings'
import { Settings } from '../../schema/settings.schema'
import { ThemeProps } from '../../../../shared/src/theme'
import { eventLogger, EventLoggerProps } from '../../tracking/eventLogger'
import { ThemePreferenceProps } from '../../theme'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { Link } from '../../../../shared/src/components/Link'
import { BrandLogo } from '../../components/branding/BrandLogo'
import { VersionContextProps } from '../../../../shared/src/search/util'
import { VersionContext } from '../../schema/site.schema'
import { ViewGrid } from '../../repo/tree/ViewGrid'
import { useObservable } from '../../../../shared/src/util/useObservable'
import { getViewsForContainer } from '../../../../shared/src/api/client/services/viewService'
import { isErrorLike } from '../../../../shared/src/util/errors'
import { ContributableViewContainer } from '../../../../shared/src/api/protocol'
import { EMPTY } from 'rxjs'
import classNames from 'classnames'
import { repogroupList, homepageLanguageList } from '../../repogroups/HomepageConfig'
import { SearchPageInput } from './SearchPageInput'
import { KeyboardShortcutsProps } from '../../keyboardShortcuts/keyboardShortcuts'

interface Props
    extends SettingsCascadeProps<Settings>,
        ThemeProps,
        ThemePreferenceProps,
        ActivationProps,
        PatternTypeProps,
        CaseSensitivityProps,
        KeyboardShortcutsProps,
        EventLoggerProps,
        ExtensionsControllerProps<'executeCommand' | 'services'>,
        PlatformContextProps<'forceUpdateTooltip' | 'settings'>,
        InteractiveSearchProps,
        SmartSearchFieldProps,
        CopyQueryButtonProps,
        VersionContextProps,
        RepogroupHomepageProps {
    authenticatedUser: GQL.IUser | null
    location: H.Location
    history: H.History
    isSourcegraphDotCom: boolean
    setVersionContext: (versionContext: string | undefined) => void
    availableVersionContexts: VersionContext[] | undefined

    // For NavLinks
    authRequired?: boolean
    showCampaigns: boolean
}

/**
 * The search page
 */
export const SearchPage: React.FunctionComponent<Props> = props => {
    useEffect(() => eventLogger.logViewEvent('Home'))

    const codeInsightsEnabled =
        !isErrorLike(props.settingsCascade.final) && !!props.settingsCascade.final?.experimentalFeatures?.codeInsights

    const views = useObservable(
        useMemo(
            () =>
                codeInsightsEnabled
                    ? getViewsForContainer(
                          ContributableViewContainer.Homepage,
                          {},
                          props.extensionsController.services.view
                      )
                    : EMPTY,
            [codeInsightsEnabled, props.extensionsController.services.view]
        )
    )

    return (
        <div className="search-page">
            <BrandLogo className="search-page__logo" isLightTheme={props.isLightTheme} />
            <div
                className={classNames('search-page__search-container', {
                    'search-page__search-container--with-repogroups': props.isSourcegraphDotCom,
                })}
            >
                <SearchPageInput {...props} source="home" />
                {views && <ViewGrid {...props} className="mt-5" views={views} />}
            </div>
            {props.isSourcegraphDotCom && props.showRepogroupHomepage && (
                <div className="search-page__repogroup-content container-fluid mt-5">
                    <div className="d-flex align-items-baseline mb-3">
                        <h3 className="search-page__help-content-header mr-2">Search in repository groups</h3>
                        <span className="text-monospace font-weight-normal search-page__lang-ref">
                            <span className="search-page__keyword-text">repogroup:</span>
                            <i>name</i>
                        </span>
                    </div>
                    <div className="search-page__repogroup-list-cards">
                        {repogroupList.map(repogroup => (
                            <div className="d-flex" key={repogroup.name}>
                                <img className="search-page__repogroup-list-icon mr-2" src={repogroup.homepageIcon} />
                                <div className="d-flex flex-column">
                                    <Link
                                        to={repogroup.url}
                                        className="search-page__repogroup-listing-title search-page__web-link font-weight-bold"
                                    >
                                        {repogroup.title}
                                    </Link>
                                    <p className="search-page__repogroup-listing-description">
                                        {repogroup.homepageDescription}
                                    </p>
                                </div>
                            </div>
                        ))}
                    </div>
                    <div className="search-page__help-content row mt-5">
                        <div className="search-page__example-searches col-xs-12 col-lg-5 col-xl-6">
                            <h3 className="search-page__help-content-header">Example searches</h3>
                            <ul className="list-group-flush p-0 mt-2">
                                <li className="list-group-item px-0 pt-3 pb-2">
                                    <Link
                                        to="/search?q=lang:javascript+alert%28:%5Bvariable%5D%29&patternType=structural"
                                        className="text-monospace mb-2"
                                    >
                                        <span className="search-page__keyword-text">lang:</span>javascript
                                        alert(:[variable])
                                    </Link>{' '}
                                    <p className="mt-2">
                                        Find usages of the alert() method that displays an alert box.
                                    </p>
                                </li>
                                <li className="list-group-item px-0 pt-3 pb-2">
                                    <Link
                                        to="/search?q=lang:python+from+%5CB%5C.%5Cw%2B+import+%5Cw%2B&patternType=regexp"
                                        className="text-monospace mb-2"
                                    >
                                        <span className="search-page__keyword-text">lang:</span>python from \B\.\w+
                                        import \w+
                                    </Link>{' '}
                                    <p className="mt-2">
                                        Search for explicit imports with one or more leading dots that indicate current
                                        and parent packages involved.
                                    </p>
                                </li>
                                <li className="list-group-item px-0 pt-3 pb-2">
                                    <Link
                                        to='/search?q=repo:%5Egithub%5C.com/golang/go%24+type:diff+after:"1+week+ago"&patternType=literal"'
                                        className="text-monospace mb-2"
                                    >
                                        <span className="search-page__keyword-text">repo:</span>
                                        ^github\.com/golang/go$ <span className="search-page__keyword-text">type:</span>
                                        diff <span className="search-page__keyword-text">after:</span>"1 week ago"
                                    </Link>{' '}
                                    <p className="mt-2">
                                        Browse diffs for recent code changes in the 'golang/go' GitHub repository.
                                    </p>
                                </li>
                                <li className="list-group-item px-0 pt-3 pb-2">
                                    <Link
                                        to='/search?q=file:pod.yaml+content:"kind:+ReplicationController"&patternType=literal'
                                        className="text-monospace mb-2"
                                    >
                                        <span className="search-page__keyword-text">file:</span>pod.yaml{' '}
                                        <span className="search-page__keyword-text">content:</span>"kind:
                                        ReplicationController"
                                    </Link>{' '}
                                    <p className="mt-2">
                                        Use a ReplicationController configuration to ensure specified number of pod
                                        replicas are running at any one time.
                                    </p>
                                </li>
                            </ul>
                        </div>
                        <div className="search-page__search-a-language col-xs-12 col-md-6 col-lg-3 col-xl-2">
                            <div className="align-items-baseline mb-4">
                                <h3 className="search-page__help-content-header">
                                    Search a language{' '}
                                    <span className="text-monospace font-weight-normal search-page__lang-ref">
                                        <span className="search-page__keyword-text ml-1">lang:</span>
                                        <i className="search-page__keyword-value-text">name</i>
                                    </span>
                                </h3>
                            </div>
                            <div className="d-flex row-cols-2 mt-2">
                                <div className="d-flex flex-column col mr-auto">
                                    {homepageLanguageList
                                        .slice(0, Math.ceil(homepageLanguageList.length / 2))
                                        .map(language => (
                                            <Link
                                                className="search-page__web-link search-page__lang-link text-monospace mb-3"
                                                to={`/search?q=lang:${language.filterName}`}
                                                key={language.name}
                                            >
                                                {language.name}
                                            </Link>
                                        ))}
                                </div>
                                <div className="d-flex flex-column col">
                                    {homepageLanguageList
                                        .slice(Math.ceil(homepageLanguageList.length / 2), homepageLanguageList.length)
                                        .map(language => (
                                            <Link
                                                className="search-page__web-link search-page__lang-link text-monospace mb-3"
                                                to={`/search?q=lang:${language.filterName}`}
                                                key={language.name}
                                            >
                                                {language.name}
                                            </Link>
                                        ))}
                                </div>
                            </div>
                        </div>
                        <div className="search-page__search-syntax col-xs-12 col-md-6  col-lg-4">
                            <h3 className="search-page__help-content-header">Search syntax</h3>
                            <div className="mt-3 row">
                                <dl className="col-xs-12 col-lg-6 mb-4">
                                    <dt className="search-page__help-content-subheading">
                                        <h5>Common search keywords</h5>
                                    </dt>
                                    <dd className="text-monospace">
                                        <p>repo:my/repo</p>
                                    </dd>
                                    <dd className="text-monospace">
                                        <p>repo:github.com/myorg/</p>
                                    </dd>
                                    <dd className="text-monospace">
                                        <p>file:my/file</p>
                                    </dd>
                                    <dd className="text-monospace">
                                        <p>lang:javascript</p>
                                    </dd>
                                    <dt className="search-page__help-content-subheading mt-5">
                                        <h5>Diff/commit search keywords</h5>
                                    </dt>
                                    <dd className="text-monospace">
                                        <p>type:diff or type:commit</p>
                                    </dd>
                                    <dd className="text-monospace">
                                        <p>after:"2 weeks ago"</p>
                                    </dd>
                                    <dd className="text-monospace">
                                        <p>author:alice@example.com</p>
                                    </dd>{' '}
                                    <dd className="text-monospace">
                                        <p>repo:r@*refs/heads/ (all branches)</p>
                                    </dd>
                                </dl>
                                <dl className="col-xs-12 col-xl-6">
                                    <dt className="search-page__help-content-subheading">
                                        <h5>Finding matches</h5>
                                    </dt>
                                    <dd>
                                        <p>
                                            <strong>Regexp:</strong>{' '}
                                            <span className="text-monospace">(read|write)File</span>
                                        </p>
                                    </dd>{' '}
                                    <dd>
                                        <p>
                                            <strong>Exact:</strong> <span className="text-monospace">"fs.open(f)"</span>
                                        </p>
                                    </dd>
                                    <dd>
                                        <p>
                                            <strong>Structural:</strong>{' '}
                                            <span className="text-monospace">if(:[my_match])</span>
                                        </p>
                                    </dd>
                                </dl>
                            </div>
                        </div>
                    </div>
                </div>
            )}
        </div>
    )
}
