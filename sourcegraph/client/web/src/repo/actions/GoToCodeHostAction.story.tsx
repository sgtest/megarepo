import { action } from '@storybook/addon-actions'
import { storiesOf } from '@storybook/react'
import { noop } from 'lodash'
import BitbucketIcon from 'mdi-react/BitbucketIcon'
import GithubIcon from 'mdi-react/GithubIcon'
import GitlabIcon from 'mdi-react/GitlabIcon'
import React, { useEffect, useState } from 'react'
import { PhabricatorIcon } from '../../../../shared/src/components/icons'
import { WebStory } from '../../components/WebStory'
import { InstallBrowserExtensionPopover } from './InstallBrowserExtensionPopover'
import { ExternalServiceKind } from '../../../../shared/src/graphql/schema'

const onClose = action('onClose')
const onRejection = action('onRejection')
const onClickInstall = action('onClickInstall')

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
                <>
                    <button className="btn" id={targetID} onClick={() => setOpen(isOpen => !isOpen)}>
                        <GithubIcon className="icon-inline" />
                    </button>
                    <InstallBrowserExtensionPopover
                        url=""
                        serviceKind={serviceKind}
                        onClose={onClose}
                        onRejection={onRejection}
                        onClickInstall={onClickInstall}
                        targetID={targetID}
                        isOpen={open}
                        toggle={noop}
                    />
                </>
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
                    <>
                        <button className="btn" id={targetID} onClick={() => setOpen(isOpen => !isOpen)}>
                            <GitlabIcon className="icon-inline" />
                        </button>
                        <InstallBrowserExtensionPopover
                            url=""
                            serviceKind={serviceKind}
                            onClose={onClose}
                            onRejection={onRejection}
                            onClickInstall={onClickInstall}
                            targetID={targetID}
                            isOpen={open}
                            toggle={noop}
                        />
                    </>
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
                    <>
                        <button className="btn" id={targetID} onClick={() => setOpen(isOpen => !isOpen)}>
                            <PhabricatorIcon className="icon-inline" />
                        </button>
                        <InstallBrowserExtensionPopover
                            url=""
                            serviceKind={serviceKind}
                            onClose={onClose}
                            onRejection={onRejection}
                            onClickInstall={onClickInstall}
                            targetID={targetID}
                            isOpen={open}
                            toggle={noop}
                        />
                    </>
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
                    <>
                        <button className="btn" id={targetID} onClick={() => setOpen(isOpen => !isOpen)}>
                            <BitbucketIcon className="icon-inline" />
                        </button>

                        <InstallBrowserExtensionPopover
                            url=""
                            serviceKind={serviceKind}
                            onClose={onClose}
                            onRejection={onRejection}
                            onClickInstall={onClickInstall}
                            targetID={targetID}
                            isOpen={open}
                            toggle={noop}
                        />
                    </>
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
