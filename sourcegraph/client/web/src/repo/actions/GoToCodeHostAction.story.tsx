/* eslint-disable react-hooks/rules-of-hooks */
import { useEffect, useState } from 'react'

import { mdiGithub, mdiGitlab, mdiBitbucket } from '@mdi/js'
import { Meta, Story, DecoratorFn } from '@storybook/react'

import { PhabricatorIcon } from '@sourcegraph/shared/src/components/icons'
import { ExternalServiceKind } from '@sourcegraph/shared/src/schema'
import { Button, Popover, PopoverTrigger, Icon } from '@sourcegraph/wildcard'

import { WebStory } from '../../components/WebStory'

const decorator: DecoratorFn = story => <div className="container mt-3">{story()}</div>

const config: Meta = {
    title: 'web/repo/actions/InstallBrowserExtensionPopover',
    decorators: [decorator],
}

export default config

export const GitHub: Story = () => (
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
                        <Icon aria-hidden="true" svgPath={mdiGithub} />
                    </PopoverTrigger>
                </Popover>
            )
        }}
    </WebStory>
)

GitHub.storyName = 'GitHub'

// Disable Chromatic for the non-GitHub popovers since they are mostly the same
export const GitLab: Story = () => (
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
                        <Icon aria-hidden={true} svgPath={mdiGitlab} />
                    </PopoverTrigger>
                </Popover>
            )
        }}
    </WebStory>
)

GitLab.storyName = 'GitLab'
GitLab.parameters = {
    chromatic: {
        disable: true,
    },
}

export const Phabricator: Story = () => (
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
                </Popover>
            )
        }}
    </WebStory>
)

Phabricator.parameters = {
    chromatic: {
        disable: true,
    },
}

export const BitbucketServer: Story = () => (
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
                        <Icon aria-hidden={true} svgPath={mdiBitbucket} />
                    </PopoverTrigger>
                </Popover>
            )
        }}
    </WebStory>
)

BitbucketServer.storyName = 'Bitbucket server'

BitbucketServer.parameters = {
    chromatic: {
        disable: true,
    },
}
