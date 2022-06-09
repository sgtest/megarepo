import { useEffect, useState } from 'react'

import { action } from '@storybook/addon-actions'
import { storiesOf } from '@storybook/react'
import BitbucketIcon from 'mdi-react/BitbucketIcon'
import GithubIcon from 'mdi-react/GithubIcon'
import GitlabIcon from 'mdi-react/GitlabIcon'

import { PhabricatorIcon } from '@sourcegraph/shared/src/components/icons'
import { ExternalServiceKind } from '@sourcegraph/shared/src/schema'
import { Button, Popover, PopoverTrigger, Icon } from '@sourcegraph/wildcard'

import { WebStory } from '../../components/WebStory'

import { InstallBrowserExtensionPopover } from './InstallBrowserExtensionPopover'

const onClose = action('onClose')
const onReject = action('onReject')
const onInstall = action('onInstall')

const { add } = storiesOf('web/repo/actions/InstallBrowserExtensionPopover', module).addDecorator(story => (
    <div className="container mt-3">{story()}</div>
))

add('GitHub', () => (
    <WebStory>
        {() => {
            const serviceKind = ExternalServiceKind.GITHUB
            const targetID = `view-on-${serviceKind}`
            const [open, setOpen] = useState(false)
            // The popover cannot be open on initial render
            // since the target element isn't in the DOM yet
            useEffect(() => {
                setTimeout(() => setOpen(true), 0)
            }, [])
            return (
                <Popover isOpen={open} onOpenChange={event => setOpen(event.isOpen)}>
                    <PopoverTrigger as={Button} id={targetID} aria-label="Github">
                        <Icon as={GithubIcon} aria-hidden="true" />
                    </PopoverTrigger>
                    <InstallBrowserExtensionPopover
                        url=""
                        serviceKind={serviceKind}
                        onClose={onClose}
                        onReject={onReject}
                        onInstall={onInstall}
                    />
                </Popover>
            )
        }}
    </WebStory>
))

// Disable Chromatic for the non-GitHub popovers since they are mostly the same

add(
    'GitLab',
    () => (
        <WebStory>
            {() => {
                const serviceKind = ExternalServiceKind.GITLAB
                const targetID = `view-on-${serviceKind}`
                const [open, setOpen] = useState(false)
                useEffect(() => {
                    setTimeout(() => setOpen(true), 0)
                }, [])
                return (
                    <Popover isOpen={open} onOpenChange={event => setOpen(event.isOpen)}>
                        <PopoverTrigger as={Button} id={targetID} aria-label="Gitlab">
                            <Icon as={GitlabIcon} aria-hidden={true} />
                        </PopoverTrigger>
                        <InstallBrowserExtensionPopover
                            url=""
                            serviceKind={serviceKind}
                            onClose={onClose}
                            onReject={onReject}
                            onInstall={onInstall}
                        />
                    </Popover>
                )
            }}
        </WebStory>
    ),
    {
        chromatic: {
            disable: true,
        },
    }
)

add(
    'Phabricator',
    () => (
        <WebStory>
            {() => {
                const serviceKind = ExternalServiceKind.PHABRICATOR
                const targetID = `view-on-${serviceKind}`
                const [open, setOpen] = useState(false)
                useEffect(() => {
                    setTimeout(() => setOpen(true), 0)
                }, [])
                return (
                    <Popover isOpen={open} onOpenChange={event => setOpen(event.isOpen)}>
                        <PopoverTrigger as={Button} id={targetID} aria-label="Phabricator">
                            <Icon as={PhabricatorIcon} aria-hidden={true} />
                        </PopoverTrigger>
                        <InstallBrowserExtensionPopover
                            url=""
                            serviceKind={serviceKind}
                            onClose={onClose}
                            onReject={onReject}
                            onInstall={onInstall}
                        />
                    </Popover>
                )
            }}
        </WebStory>
    ),
    {
        chromatic: {
            disable: true,
        },
    }
)

add(
    'Bitbucket server',
    () => (
        <WebStory>
            {() => {
                const serviceKind = ExternalServiceKind.BITBUCKETSERVER
                const targetID = `view-on-${serviceKind}`
                const [open, setOpen] = useState(false)
                useEffect(() => {
                    setTimeout(() => setOpen(true), 0)
                }, [])
                return (
                    <Popover isOpen={open} onOpenChange={event => setOpen(event.isOpen)}>
                        <PopoverTrigger as={Button} id={targetID} aria-label="Bitbucket">
                            <Icon as={BitbucketIcon} aria-hidden={true} />
                        </PopoverTrigger>
                        <InstallBrowserExtensionPopover
                            url=""
                            serviceKind={serviceKind}
                            onClose={onClose}
                            onReject={onReject}
                            onInstall={onInstall}
                        />
                    </Popover>
                )
            }}
        </WebStory>
    ),
    {
        chromatic: {
            disable: true,
        },
    }
)
