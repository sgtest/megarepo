import React, { useEffect, useState } from 'react'

import {
    mdiClose,
    mdiCogOutline,
    mdiDelete,
    mdiDotsVertical,
    mdiOpenInNew,
    mdiPlus,
    mdiChevronRight,
    mdiViewList,
    mdiFormatListBulleted,
} from '@mdi/js'
import classNames from 'classnames'
import { useLocation, useNavigate } from 'react-router-dom'

import { CodyLogo } from '@sourcegraph/cody-ui/src/icons/CodyLogo'
import { AuthenticatedUser } from '@sourcegraph/shared/src/auth'
import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary'
import {
    Badge,
    Button,
    Icon,
    Menu,
    MenuButton,
    MenuList,
    MenuDivider,
    MenuItem,
    MenuLink,
    PageHeader,
    Link,
    H4,
    H3,
    Text,
    ButtonLink,
    Tooltip,
} from '@sourcegraph/wildcard'

import { MarketingBlock } from '../../components/MarketingBlock'
import { Page } from '../../components/Page'
import { PageTitle } from '../../components/PageTitle'
import { SourcegraphContext } from '../../jscontext'
import { eventLogger } from '../../tracking/eventLogger'
import { EventName } from '../../util/constants'
import { ChatUI } from '../components/ChatUI'
import { CodyMarketingPage } from '../components/CodyMarketingPage'
import { HistoryList } from '../components/HistoryList'
import { isCodyEnabled } from '../isCodyEnabled'
import { CodyChatStore, useCodyChat } from '../useCodyChat'

import { CodyColorIcon } from './CodyPageIcon'

import styles from './CodyChatPage.module.scss'

interface CodyChatPageProps {
    isSourcegraphDotCom: boolean
    authenticatedUser: AuthenticatedUser | null
    isSourcegraphApp: boolean
    context: Pick<SourcegraphContext, 'authProviders'>
}

const onDownloadVSCodeClick = (): void => eventLogger.log(EventName.CODY_CHAT_DOWNLOAD_VSCODE)
const onTryOnPublicCodeClick = (): void => eventLogger.log(EventName.CODY_CHAT_TRY_ON_PUBLIC_CODE)

const transcriptIdFromUrl = (pathname: string): string | undefined => {
    const serializedID = pathname.split('/').pop()
    if (!serializedID) {
        return
    }

    try {
        return atob(serializedID)
    } catch {
        return
    }
}

const onTranscriptHistoryLoad = (
    loadTranscriptFromHistory: CodyChatStore['loadTranscriptFromHistory'],
    transcriptHistory: CodyChatStore['transcriptHistory'],
    initializeNewChat: CodyChatStore['initializeNewChat']
): void => {
    if (transcriptHistory.length > 0) {
        const transcriptId = transcriptIdFromUrl(window.location.pathname)

        if (transcriptId && transcriptHistory.find(({ id }) => id === transcriptId)) {
            loadTranscriptFromHistory(transcriptId).catch(() => null)
        } else {
            loadTranscriptFromHistory(transcriptHistory[0].id).catch(() => null)
        }
    } else {
        initializeNewChat()
    }
}

export const CodyChatPage: React.FunctionComponent<CodyChatPageProps> = ({
    authenticatedUser,
    context,
    isSourcegraphDotCom,
    isSourcegraphApp,
}) => {
    const { pathname } = useLocation()
    const navigate = useNavigate()

    const codyChatStore = useCodyChat({
        onTranscriptHistoryLoad,
        autoLoadTranscriptFromHistory: false,
    })
    const {
        initializeNewChat,
        clearHistory,
        loaded,
        transcript,
        transcriptHistory,
        loadTranscriptFromHistory,
        deleteHistoryItem,
    } = codyChatStore
    const [showVSCodeCTA] = useState<boolean>(Math.random() < 0.5 || true)
    const [isCTADismissed = true, setIsCTADismissed] = useTemporarySetting('cody.chatPageCta.dismissed', false)
    const onCTADismiss = (): void => setIsCTADismissed(true)

    useEffect(() => {
        eventLogger.log(EventName.CODY_CHAT_PAGE_VIEWED)
    }, [])

    const transcriptId = transcript?.id

    useEffect(() => {
        if (!loaded || !transcriptId) {
            return
        }
        const idFromUrl = transcriptIdFromUrl(pathname)

        if (transcriptId !== idFromUrl) {
            navigate(`/cody/${btoa(transcriptId)}`, {
                replace: true,
            })
        }
    }, [transcriptId, loaded, pathname, navigate])

    const [showMobileHistory, setShowMobileHistory] = useState<boolean>(false)
    // Close mobile history list when transcript changes
    useEffect(() => {
        setShowMobileHistory(false)
    }, [transcript])

    if (!loaded) {
        return null
    }

    if (!authenticatedUser || !isCodyEnabled()) {
        return <CodyMarketingPage isSourcegraphDotCom={isSourcegraphDotCom} context={context} />
    }

    return (
        <Page className={classNames('d-flex flex-column', styles.page)}>
            <PageTitle title="Cody AI Chat" />
            {!isSourcegraphDotCom && !isCTADismissed && (
                <MarketingBlock
                    wrapperClassName="mb-5"
                    contentClassName={classNames(styles.ctaWrapper, styles.ctaContent)}
                >
                    <div className="d-flex">
                        <CodyCTAIcon className="flex-shrink-0" />
                        <div className="ml-3">
                            <H3>Cody is more powerful in your IDE</H3>
                            <Text>
                                Cody adds powerful AI assistant functionality like inline completions and assist, and
                                powerful recipes to help you understand codebases and generate and fix code more
                                accurately.
                            </Text>
                            <ButtonLink variant="primary" to="/help/cody#get-cody">
                                View editor extensions &rarr;
                            </ButtonLink>
                        </div>
                    </div>
                    <Icon
                        svgPath={mdiClose}
                        aria-label="Close Cody editor extensions CTA"
                        className={classNames(styles.closeButton, 'position-absolute m-0')}
                        onClick={onCTADismiss}
                    />
                </MarketingBlock>
            )}
            <PageHeader
                actions={
                    <div className="d-flex">
                        <Button variant="primary" onClick={initializeNewChat}>
                            <Icon aria-hidden={true} svgPath={mdiPlus} />
                            New chat
                        </Button>
                    </div>
                }
                description={
                    <>
                        Cody answers code questions and writes code for you by leveraging your entire codebase and the
                        code graph.
                        {!isSourcegraphDotCom && isCTADismissed && (
                            <>
                                {' '}
                                <Link to="/help/cody#get-cody">Cody is more powerful in the IDE</Link>.
                            </>
                        )}
                    </>
                }
                className={styles.pageHeader}
            >
                <PageHeader.Heading as="h2" styleAs="h1">
                    <PageHeader.Breadcrumb icon={CodyColorIcon}>
                        <div className="d-inline-flex align-items-center">
                            Cody Chat
                            {!isSourcegraphApp && (
                                <Badge variant="info" className="ml-2">
                                    Beta
                                </Badge>
                            )}
                        </div>
                    </PageHeader.Breadcrumb>
                </PageHeader.Heading>
            </PageHeader>
            {/* Page content */}
            <div className={classNames('row flex-1 overflow-hidden', styles.pageWrapper)}>
                <div className={classNames('col-md-3', styles.sidebarWrapper)}>
                    <div className={styles.sidebarHeader}>
                        <H4>
                            <b>Chats</b>
                        </H4>
                        <Menu>
                            <MenuButton variant="icon" outline={false}>
                                <Icon aria-hidden={true} svgPath={mdiDotsVertical} size="md" />
                            </MenuButton>

                            <MenuList>
                                <MenuItem onSelect={clearHistory}>
                                    <Icon aria-hidden={true} svgPath={mdiDelete} /> Clear all chats
                                </MenuItem>
                                <MenuDivider />
                                <MenuLink as={Link} to="/help/cody" target="_blank" rel="noopener">
                                    <Icon aria-hidden={true} svgPath={mdiOpenInNew} /> Cody Docs & FAQ
                                </MenuLink>
                                {authenticatedUser?.siteAdmin && (
                                    <MenuLink as={Link} to="/site-admin/cody">
                                        <Icon aria-hidden={true} svgPath={mdiCogOutline} /> Cody Settings
                                    </MenuLink>
                                )}
                            </MenuList>
                        </Menu>
                    </div>
                    <div className={classNames('h-100 mb-4', styles.sidebar)}>
                        <HistoryList
                            currentTranscript={transcript}
                            transcriptHistory={transcriptHistory}
                            truncateMessageLength={60}
                            loadTranscriptFromHistory={loadTranscriptFromHistory}
                            deleteHistoryItem={deleteHistoryItem}
                        />
                    </div>
                    {isSourcegraphDotCom &&
                        !isCTADismissed &&
                        (showVSCodeCTA ? (
                            <MarketingBlock
                                wrapperClassName="d-flex"
                                contentClassName={classNames(
                                    'flex-grow-1 d-flex flex-column justify-content-between',
                                    styles.ctaWrapper
                                )}
                            >
                                <H3 className="d-flex align-items-center mb-4">Try the VS Code Extension</H3>
                                <Text>
                                    This extension combines an LLM with the context of your code to help you generate
                                    and fix code.
                                </Text>
                                <div className="mb-2">
                                    <Link
                                        to="https://marketplace.visualstudio.com/items?itemName=sourcegraph.cody-ai"
                                        className={classNames(
                                            'd-inline-flex align-items-center text-merged',
                                            styles.ctaLink
                                        )}
                                        onClick={onDownloadVSCodeClick}
                                    >
                                        Download the VS Code Extension
                                        <Icon svgPath={mdiChevronRight} aria-hidden={true} />
                                    </Link>
                                </div>
                                <img
                                    src="https://storage.googleapis.com/sourcegraph-assets/TryCodyVSCodeExtension.png"
                                    alt="Try Cody VS Code Extension"
                                    width={666}
                                />
                                <Icon
                                    svgPath={mdiClose}
                                    aria-label="Close try Cody widget"
                                    className={classNames(styles.closeButton, 'position-absolute m-0')}
                                    onClick={onCTADismiss}
                                />
                            </MarketingBlock>
                        ) : (
                            <MarketingBlock
                                wrapperClassName="d-flex"
                                contentClassName={classNames(
                                    'flex-grow-1 d-flex flex-column justify-content-between',
                                    styles.ctaWrapper
                                )}
                            >
                                <H3 className="d-flex align-items-center mb-4">Try Cody on Public Code</H3>
                                <Text>
                                    Cody explains, generates, and translates code within specific files and
                                    repositories.
                                </Text>
                                <div className="mb-2">
                                    <Link
                                        to="https://sourcegraph.com/github.com/openai/openai-cookbook/-/blob/apps/file-q-and-a/nextjs-with-flask-server/server/answer_question.py"
                                        className={classNames(
                                            'd-inline-flex align-items-center text-merged',
                                            styles.ctaLink
                                        )}
                                        onClick={onTryOnPublicCodeClick}
                                    >
                                        Try on a file, or repository
                                        <Icon svgPath={mdiChevronRight} aria-hidden={true} />
                                    </Link>
                                </div>
                                <img
                                    src="https://storage.googleapis.com/sourcegraph-assets/TryCodyOnPublicCode.png"
                                    alt="Try Cody on Public Code"
                                    width={666}
                                />
                                <Icon
                                    svgPath={mdiClose}
                                    aria-label="Close try Cody widget"
                                    className={classNames(styles.closeButton, 'position-absolute m-0')}
                                    onClick={onCTADismiss}
                                />
                            </MarketingBlock>
                        ))}
                </div>

                {isSourcegraphApp ? (
                    <>
                        <div
                            className={classNames(
                                'col-md-9 h-100',
                                styles.chatMainWrapper,
                                showMobileHistory && styles.chatMainWrapperWithMobileHistory
                            )}
                        >
                            <div className={styles.mobileTopBar}>
                                <Button
                                    variant="icon"
                                    className={styles.mobileTopBarButton}
                                    onClick={() => setShowMobileHistory(true)}
                                >
                                    <Icon aria-hidden={true} svgPath={mdiViewList} className="mr-2" />
                                    All chats
                                </Button>
                                <div className={classNames('border-right', styles.mobileTopBarDivider)} />
                                <Button
                                    variant="icon"
                                    className={styles.mobileTopBarButton}
                                    onClick={initializeNewChat}
                                >
                                    <Icon aria-hidden={true} svgPath={mdiPlus} className="mr-2" />
                                    New chat
                                </Button>
                            </div>
                            <ChatUI codyChatStore={codyChatStore} />
                        </div>

                        {showMobileHistory && (
                            <div className={styles.mobileHistoryWrapper}>
                                <div className={styles.mobileTopBar}>
                                    <Button
                                        variant="icon"
                                        className={classNames('w-100', styles.mobileTopBarButton)}
                                        onClick={() => setShowMobileHistory(false)}
                                    >
                                        <Icon aria-hidden={true} svgPath={mdiClose} className="mr-2" />
                                        Close
                                    </Button>
                                </div>
                                <div className={styles.mobileHistory}>
                                    <HistoryList
                                        currentTranscript={transcript}
                                        transcriptHistory={transcriptHistory}
                                        truncateMessageLength={60}
                                        loadTranscriptFromHistory={loadTranscriptFromHistory}
                                        deleteHistoryItem={deleteHistoryItem}
                                    />
                                </div>
                            </div>
                        )}
                    </>
                ) : (
                    <div className={classNames('col-md-9 h-100', styles.chatMainWrapper)}>
                        <div className={styles.mobileTopBarWeb}>
                            <div className="d-flex col-2 p-0">
                                <Tooltip content="Chat history">
                                    <Button
                                        variant="icon"
                                        className="mr-2"
                                        aria-label="Active chats"
                                        onClick={() => setShowMobileHistory(true)}
                                        aria-pressed={showMobileHistory}
                                    >
                                        <Icon aria-hidden={true} svgPath={mdiFormatListBulleted} />
                                    </Button>
                                </Tooltip>
                                <Tooltip content="Start a new chat">
                                    <Button variant="icon" aria-label="Start a new chat" onClick={initializeNewChat}>
                                        <Icon aria-hidden={true} svgPath={mdiPlus} />
                                    </Button>
                                </Tooltip>
                                {showMobileHistory && (
                                    <Tooltip content="Clear all chats">
                                        <Button
                                            variant="icon"
                                            className="ml-2"
                                            aria-label="Clear all chats"
                                            onClick={clearHistory}
                                        >
                                            <Icon aria-hidden={true} svgPath={mdiDelete} />
                                        </Button>
                                    </Tooltip>
                                )}
                            </div>
                            <div className="col-8 d-flex justify-content-center">
                                <div className="d-flex flex-shrink-0 align-items-center">
                                    <CodyLogo />
                                    {showMobileHistory ? 'Chats' : 'Ask Cody'}
                                    <div className="ml-2">
                                        <Badge variant="info">Beta</Badge>
                                    </div>
                                </div>
                            </div>
                            <div className="col-2 d-flex" />
                        </div>
                        {showMobileHistory ? (
                            <HistoryList
                                currentTranscript={transcript}
                                transcriptHistory={transcriptHistory}
                                truncateMessageLength={60}
                                loadTranscriptFromHistory={loadTranscriptFromHistory}
                                deleteHistoryItem={deleteHistoryItem}
                            />
                        ) : (
                            <ChatUI codyChatStore={codyChatStore} />
                        )}
                    </div>
                )}
            </div>
        </Page>
    )
}

const CodyCTAIcon: React.FunctionComponent<{ className?: string }> = ({ className }) => (
    <svg
        width="146"
        height="112"
        viewBox="0 0 146 112"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className={className}
    >
        <rect x="24" y="24" width="98" height="64" rx="6" fill="#E8D1FF" />
        <path
            d="M56.25 65.3333C56.25 65.687 56.3817 66.0261 56.6161 66.2761C56.8505 66.5262 57.1685 66.6667 57.5 66.6667H60V69.3333H56.875C56.1875 69.3333 55 68.7333 55 68C55 68.7333 53.8125 69.3333 53.125 69.3333H50V66.6667H52.5C52.8315 66.6667 53.1495 66.5262 53.3839 66.2761C53.6183 66.0261 53.75 65.687 53.75 65.3333V46.6667C53.75 46.313 53.6183 45.9739 53.3839 45.7239C53.1495 45.4738 52.8315 45.3333 52.5 45.3333H50V42.6667H53.125C53.8125 42.6667 55 43.2667 55 44C55 43.2667 56.1875 42.6667 56.875 42.6667H60V45.3333H57.5C57.1685 45.3333 56.8505 45.4738 56.6161 45.7239C56.3817 45.9739 56.25 46.313 56.25 46.6667V65.3333Z"
            fill="#A305E1"
        />
        <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M88.9095 45C90.3781 45 91.5686 46.1789 91.5686 47.6331V52.314C91.5686 53.7682 90.3781 54.9471 88.9095 54.9471C87.4409 54.9471 86.2504 53.7682 86.2504 52.314V47.6331C86.2504 46.1789 87.4409 45 88.9095 45Z"
            fill="#A305E1"
        />
        <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M72.068 51.1437C72.068 49.6895 73.2585 48.5106 74.7271 48.5106H79.4544C80.923 48.5106 82.1135 49.6895 82.1135 51.1437C82.1135 52.5978 80.923 53.7767 79.4544 53.7767H74.7271C73.2585 53.7767 72.068 52.5978 72.068 51.1437Z"
            fill="#A305E1"
        />
        <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M95.2643 58.8091C96.2107 59.6994 96.2491 61.1808 95.35 62.1179L94.5134 62.99C87.9666 69.8138 76.9295 69.6438 70.6002 62.6216C69.731 61.6572 69.8159 60.1777 70.7898 59.317C71.7637 58.4563 73.2579 58.5403 74.1271 59.5047C78.6157 64.4848 86.4432 64.6053 91.0861 59.7659L91.9227 58.8939C92.8218 57.9568 94.3179 57.9188 95.2643 58.8091Z"
            fill="#A305E1"
        />
    </svg>
)
