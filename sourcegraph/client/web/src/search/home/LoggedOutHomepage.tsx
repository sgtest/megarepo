import React from 'react'

import classNames from 'classnames'

import { ModalVideo } from '@sourcegraph/search-ui'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { Link } from '@sourcegraph/wildcard'

import { communitySearchContextsList } from '../../communitySearchContexts/HomepageConfig'
import { FeatureFlagProps } from '../../featureFlags/featureFlags'
import { GettingStartedTour } from '../../tour/GettingStartedTour'

import { CustomersSection } from './CustomersSection'
import { DynamicWebFonts } from './DynamicWebFonts'
import { HeroSection } from './HeroSection'
import { exampleTripsAndTricks, fonts } from './LoggedOutHomepage.constants'
import { SelfHostInstructions } from './SelfHostInstructions'
import { TipsAndTricks } from './TipsAndTricks'

import styles from './LoggedOutHomepage.module.scss'

export interface LoggedOutHomepageProps extends TelemetryProps, ThemeProps, FeatureFlagProps {}

export const LoggedOutHomepage: React.FunctionComponent<LoggedOutHomepageProps> = props => (
    <DynamicWebFonts fonts={fonts}>
        <div className={styles.loggedOutHomepage}>
            <div className={styles.content}>
                <GettingStartedTour
                    height={8}
                    className={classNames(styles.gettingStartedTour, 'h-100')}
                    telemetryService={props.telemetryService}
                    featureFlags={props.featureFlags}
                    isSourcegraphDotCom={true}
                />
                <div className={styles.videoCard}>
                    <div className={classNames(styles.title, 'mb-2')}>Watch and learn</div>
                    <ModalVideo
                        id="three-ways-to-search-title"
                        title="Three ways to search"
                        src="https://www.youtube-nocookie.com/embed/XLfE2YuRwvw"
                        showCaption={true}
                        thumbnail={{
                            src: `img/watch-and-learn-${props.isLightTheme ? 'light' : 'dark'}.png`,
                            alt: 'Watch and learn video thumbnail',
                        }}
                        onToggle={isOpen =>
                            props.telemetryService.log(
                                isOpen ? 'HomepageVideoWaysToSearchClicked' : 'HomepageVideoClosed'
                            )
                        }
                        assetsRoot={window.context?.assetsRoot || ''}
                    />
                </div>

                <TipsAndTricks
                    title="Tips and Tricks"
                    examples={exampleTripsAndTricks}
                    moreLink={{
                        label: 'More search features',
                        href: 'https://docs.sourcegraph.com/code_search/explanations/features',
                        trackEventName: 'HomepageExampleMoreSearchFeaturesClicked',
                    }}
                    {...props}
                />
            </div>

            <div className={styles.heroSection}>
                <HeroSection {...props} />
            </div>

            <div className={styles.communitySearchContextsSection}>
                <div className="d-block d-md-flex align-items-baseline mb-3">
                    <div className={classNames(styles.title, 'mr-2')}>Search open source communities</div>
                    <div className="font-weight-normal text-muted">
                        Customized search portals for our open source partners
                    </div>
                </div>
                <div className={styles.loggedOutHomepageCommunitySearchContextListCards}>
                    {communitySearchContextsList.map(communitySearchContext => (
                        <div
                            className={classNames(
                                styles.loggedOutHomepageCommunitySearchContextListCard,
                                'd-flex align-items-center'
                            )}
                            key={communitySearchContext.spec}
                        >
                            <img
                                className={classNames(styles.loggedOutHomepageCommunitySearchContextListIcon, 'mr-2')}
                                src={communitySearchContext.homepageIcon}
                                alt={`${communitySearchContext.spec} icon`}
                            />
                            <Link
                                to={communitySearchContext.url}
                                className={classNames(styles.loggedOutHomepageCommunitySearchContextsListingTitle)}
                            >
                                {communitySearchContext.title}
                            </Link>
                        </div>
                    ))}
                </div>
            </div>

            <div className={styles.selfHostSection}>
                <SelfHostInstructions {...props} />
            </div>

            <div className={styles.customerSection}>
                <CustomersSection {...props} />
            </div>
        </div>
    </DynamicWebFonts>
)
