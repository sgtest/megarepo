import { useEffect } from 'react'

import {
    mdiEmailOutline,
    mdiMicrosoftVisualStudioCode,
    mdiChevronRight,
    mdiApple,
    mdiLinux,
    mdiMicrosoftWindows,
} from '@mdi/js'
import classNames from 'classnames'

import { AuthenticatedUser } from '@sourcegraph/shared/src/auth'
import { Badge, H2, Icon, Link, PageHeader, Text } from '@sourcegraph/wildcard'

import { ExternalsAuth } from '../auth/components/ExternalsAuth'
import { CodyLetsWorkIcon, CodyStartCoding } from '../cody/chat/CodyPageIcon'
import { Page } from '../components/Page'
import { PageTitle } from '../components/PageTitle'
import { SourcegraphContext } from '../jscontext'
import { eventLogger } from '../tracking/eventLogger'
import { EventName } from '../util/constants'

import { DownloadAppButton } from './DownloadAppButton'
import { BackgroundImage, IntellijIcon, EmacsIcon, NeovimIcon } from './GetCodyPageIcon'

import styles from './GetCodyPage.module.scss'

interface WaitListButtonProps {
    title: string
    to: string
    icon: React.ReactElement
}

interface GetCodyPageProps {
    authenticatedUser: AuthenticatedUser | null
    context: Pick<SourcegraphContext, 'authProviders'>
}

const SOURCEGRAPH_MAC_SILICON = 'https://sourcegraph.com/.api/app/latest?arch=aarch64&target=darwin'

const SOURCEGRAPH_MAC_INTEL = 'https://sourcegraph.com/.api/app/latest?arch=x86_64&target=darwin'

const SOURCEGRAPH_LINUX = 'https://sourcegraph.com/.api/app/latest?arch=x86_64&target=linux'

const onClickCTAButton = (type: string): void =>
    eventLogger.log(EventName.SIGNUP_INITIATED, { type, source: 'get-started' })

const logEvent = (eventName: string, type?: string, source?: string): void =>
    eventLogger.log(eventName, { type, source })

export const GetCodyPage: React.FunctionComponent<GetCodyPageProps> = ({ authenticatedUser, context }) => {
    useEffect(() => {
        logEvent(EventName.VIEW_GET_CODY)
    }, [])

    return (
        <div className={styles.pageWrapper}>
            <Page className={styles.page}>
                <PageTitle title="Get Started with Cody" />
                <PageHeader className={styles.pageHeader}>
                    <Link to="https://about.sourcegraph.com/">
                        <img
                            className={styles.pageHeaderImage}
                            src="https://sourcegraph.com/.assets/img/sourcegraph-logo-dark.svg"
                            alt="Sourcegraph logo"
                        />
                    </Link>
                </PageHeader>

                <div className={styles.getStartedWithCody}>
                    <div>
                        <Text className={styles.getStartedWithCodyTitle}>Get started with Cody</Text>
                        <Text className={styles.getStartedWithCodyDescription}>
                            Try Cody free on your local machine with the Cody app and IDE extensions.
                        </Text>
                    </div>
                    <CodyLetsWorkIcon className={styles.codyLetsWorkImage} />
                </div>

                <div className={styles.cardWrapper}>
                    {/* connect to cody section */}
                    {!authenticatedUser && (
                        <div className={classNames(styles.card, 'get-cody-step', styles.focusBackground)}>
                            <H2 className={classNames(styles.cardTitle, 'mb-4')}>
                                You’ll need a Sourcegraph account to connect to Cody
                            </H2>
                            <Text className={classNames(styles.cardDescription, 'mb-4')}>
                                Log in or sign up for a Sourcegraph.com account.
                            </Text>
                            <div className={styles.authButtonsWrap}>
                                <div className={styles.externalAuthWrapper}>
                                    <ExternalsAuth
                                        context={context}
                                        githubLabel="GitHub"
                                        gitlabLabel="Gitlab"
                                        withCenteredText={true}
                                        onClick={onClickCTAButton}
                                        ctaClassName={styles.authButton}
                                        iconClassName={styles.buttonIcon}
                                        redirect="/get-cody"
                                    />
                                </div>
                                <Link
                                    to="https://sourcegraph.com/sign-up?showEmail=true&returnTo=get-cody"
                                    className={classNames('text-decoration-none', styles.emailAuthButton)}
                                    onClick={() => onClickCTAButton('builtin')}
                                >
                                    <Icon
                                        className="mr-1"
                                        svgPath={mdiEmailOutline}
                                        inline={false}
                                        aria-hidden={true}
                                    />
                                    Continue with email
                                </Link>
                            </div>
                            <Text className={styles.terms}>
                                By registering, you agree to our{' '}
                                <Link
                                    to="https://about.sourcegraph.com/terms"
                                    className={styles.termsLink}
                                    target="_blank"
                                    rel="noopener"
                                >
                                    Terms of Service
                                </Link>{' '}
                                and{' '}
                                <Link
                                    to="https://about.sourcegraph.com/terms/privacy"
                                    className={styles.termsLink}
                                    target="_blank"
                                    rel="noopener"
                                >
                                    Privacy Policy
                                </Link>
                            </Text>
                        </div>
                    )}

                    {/* Install cody desktop app section */}
                    <div className={classNames(styles.card, 'get-cody-step')}>
                        <H2 className={classNames(styles.cardTitle, 'mb-4')}>
                            Install the <span className={styles.installCodyTitle}>Cody desktop app</span>
                        </H2>
                        <Text className={styles.cardDescription}>
                            The Cody app, when combined with a Cody IDE extension, enables context fetching for all of
                            your local repositories. Without the app, Cody only fetches context on the repository
                            currently open in your IDE.
                        </Text>
                        <div className={styles.downloadButtonWrapper}>
                            <div className={classNames('d-flex flex-row flex-wrap', styles.downloadMacWrapper)}>
                                <DownloadAppButton
                                    to={SOURCEGRAPH_MAC_SILICON}
                                    buttonText="Download for Mac (Apple Silicon)"
                                    badgeText="Beta"
                                    eventName={EventName.DOWNLOAD_APP}
                                    eventType="Mac Silicon"
                                    icon={mdiApple}
                                />
                                <DownloadAppButton
                                    to={SOURCEGRAPH_MAC_INTEL}
                                    buttonText="Download for Mac (Intel)"
                                    badgeText="Beta"
                                    eventName={EventName.DOWNLOAD_APP}
                                    eventType="Mac Intel"
                                    icon={mdiApple}
                                />
                            </div>

                            <DownloadAppButton
                                to={SOURCEGRAPH_LINUX}
                                buttonText="Download for Linux"
                                badgeText="Beta"
                                eventName={EventName.DOWNLOAD_APP}
                                eventType="Linux"
                                icon={mdiLinux}
                            />

                            <Link to="/help/app" className={styles.otherOptions} target="_blank" rel="noopener">
                                Other options
                            </Link>

                            <Text className={styles.terms}>
                                By using Sourcegraph, you agree to the{' '}
                                <Link
                                    to="https://about.sourcegraph.com/terms/privacy"
                                    className={styles.termsLink}
                                    target="_blank"
                                    rel="noopener"
                                >
                                    privacy policy
                                </Link>{' '}
                                and{' '}
                                <Link
                                    to="https://about.sourcegraph.com/terms"
                                    className={styles.termsLink}
                                    target="_blank"
                                    rel="noopener"
                                >
                                    terms
                                </Link>
                            </Text>

                            <Text className={classNames('text-decoration-none', styles.downloadForWindows)}>
                                <Icon
                                    className="mr-2"
                                    svgPath={mdiMicrosoftWindows}
                                    inline={false}
                                    aria-hidden={true}
                                />
                                Download for Windows{' '}
                                <Badge className={classNames('ml-2', styles.badge)}>Coming soon</Badge>
                            </Text>

                            <Text className={styles.downloadForWindowsDescription}>
                                While the Cody app is not yet available for Windows, you can use the{' '}
                                <Link
                                    to="vscode:extension/sourcegraph.cody-ai"
                                    className={styles.downloadForWindowsDescriptionLink}
                                >
                                    {' '}
                                    Cody extension for VS Code
                                </Link>
                            </Text>
                        </div>
                    </div>

                    {/* Install cody extension section */}
                    <div className={classNames(styles.card, 'get-cody-step')}>
                        <H2 className={classNames(styles.cardTitle, 'mb-4')}>
                            Install the Cody extension for your IDE(s)
                        </H2>
                        <Text className={styles.cardDescription}>
                            If you’ve downloaded the app, it will prompt you to sign in to your Sourcegraph.com account,
                            connect your repositories, and connect your IDE extensions.
                        </Text>
                        <div className={classNames(styles.downloadBtnWrapper)}>
                            <div className={classNames(styles.downloadBtn)}>
                                <Link
                                    to="vscode:extension/sourcegraph.cody-ai"
                                    className={classNames('text-decoration-none', styles.downloadForVscode)}
                                    onClick={() => logEvent(EventName.DOWNLOAD_IDE, 'VS Code')}
                                >
                                    <Icon
                                        className={styles.vscodeIcon}
                                        svgPath={mdiMicrosoftVisualStudioCode}
                                        inline={false}
                                        aria-hidden={true}
                                    />{' '}
                                    <span className={styles.downloadForVscodeText}>Install Cody for VS Code</span>
                                </Link>
                                <Link
                                    to="https://marketplace.visualstudio.com/items?itemName=sourcegraph.cody-ai#:~:text=Cody%20for%20VS%20Code%20is,no%20problem%20for%20Cody."
                                    className={classNames('text-decoration-none', styles.vscodeMarketplace)}
                                >
                                    Or download on the VS Code Marketplace
                                    <Icon
                                        className="ml-2"
                                        svgPath={mdiChevronRight}
                                        inline={false}
                                        aria-hidden={true}
                                    />
                                </Link>
                            </div>
                            <div className={classNames(styles.downloadBtn)}>
                                <Link
                                    to="https://plugins.jetbrains.com/plugin/9682-sourcegraph"
                                    className={classNames('text-decoration-none', styles.downloadForVscode)}
                                    onClick={() => logEvent(EventName.DOWNLOAD_IDE, 'IntelliJ')}
                                >
                                    <span className={styles.vscodeIcon}>
                                        <IntellijIcon className={styles.joinWaitlistButtonIcon} />
                                    </span>
                                    <span className={styles.downloadForVscodeText}>Install Cody for Intellij</span>
                                </Link>
                            </div>
                        </div>
                        <div className={styles.comingSoonWrapper}>
                            <Text className={styles.comingSoonWrapperText}>Coming soon:</Text>
                            <div className={styles.joinWaitlistButtonWrapper}>
                                <WaitListButton
                                    to="https://info.sourcegraph.com/waitlist"
                                    icon={<NeovimIcon className={styles.joinWaitlistButtonIcon} />}
                                    title="Neovim"
                                />
                                <WaitListButton
                                    to="https://info.sourcegraph.com/waitlist"
                                    icon={<EmacsIcon className={styles.joinWaitlistButtonIcon} />}
                                    title="Emacs"
                                />
                            </div>
                        </div>
                    </div>

                    <div className={classNames(styles.card, styles.startCodingWithMeImgWrapper, 'get-cody-step')}>
                        <CodyStartCoding className={styles.startCodingWithMeImg} />
                    </div>

                    {/* cody for enterprise section */}
                    <div className={styles.codyForEnterprise}>
                        <H2 className={classNames(styles.cardTitle, styles.title)}>
                            Interested in Cody for Sourcegraph Enterprise?
                        </H2>
                        <Text className={styles.cardDescription}>
                            If you’ve downloaded the app, it will prompt you to sign in to your Sourcegraph.com account,
                            connect your repositories, and connect your IDE extensions.
                        </Text>
                        <Text className={classNames(styles.cardDescription, styles.getInTouchText)}>
                            Get in touch with our team to try Cody for Sourcegraph Enterprise.
                        </Text>
                        <div className={styles.ctaButtonWrapper}>
                            <Link
                                to="https://about.sourcegraph.com/cody/pricing"
                                className={classNames('text-decoration-none', styles.getCodyEnterpriseButton)}
                            >
                                Get Cody for Enterprise
                            </Link>
                            <Link
                                to="https://info.sourcegraph.com/talk-to-a-developer"
                                className={classNames('text-decoration-none', styles.talkToEngineerButton)}
                            >
                                Talk to an engineer
                            </Link>
                        </div>
                    </div>
                </div>

                <BackgroundImage className={styles.backgroundImage} />

                {/* footer section */}
                <div className={styles.footer}>
                    <Text>© 2023 Sourcegraph, Inc. </Text>
                    <Link
                        to="https://about.sourcegraph.com/terms"
                        className={styles.footerLink}
                        target="_blank"
                        rel="noopener"
                    >
                        Terms
                    </Link>
                    <Link
                        to="https://about.sourcegraph.com/security"
                        className={styles.footerLink}
                        target="_blank"
                        rel="noopener"
                    >
                        Security
                    </Link>
                    <Link
                        to="https://about.sourcegraph.com/terms/privacy"
                        className={styles.footerLink}
                        target="_blank"
                        rel="noopener"
                    >
                        Privacy
                    </Link>
                </div>
            </Page>
        </div>
    )
}

const WaitListButton: React.FunctionComponent<WaitListButtonProps> = ({ title, to, icon }) => (
    <div className={classNames('text-decoration-none', styles.joinWaitlistButton)}>
        {icon}
        <span className={styles.joinWaitlistButtonTitle}>{title}</span>
        <Link
            to={to}
            onClick={() => logEvent(EventName.JOIN_IDE_WAITLIST, title)}
            target="_blank"
            className={styles.joinWaitlistButtonLinkText}
            rel="noopener"
        >
            Join the waitlist
            <Icon className="ml-2" svgPath={mdiChevronRight} inline={false} aria-hidden={true} />
        </Link>
    </div>
)
