import React, { useCallback, useState } from 'react'

import copy from 'copy-to-clipboard'
import { noop } from 'lodash'
import ContentCopyIcon from 'mdi-react/ContentCopyIcon'

import { Button, TextArea, Link, Icon } from '@sourcegraph/wildcard'

import { ExternalServiceKind } from '../../../graphql-operations'

const configInstructionLinks: Record<ExternalServiceKind, string> = {
    [ExternalServiceKind.GITHUB]:
        'https://docs.github.com/en/github/authenticating-to-github/adding-a-new-ssh-key-to-your-github-account',
    [ExternalServiceKind.GITLAB]: 'https://docs.gitlab.com/ee/ssh/#add-an-ssh-key-to-your-gitlab-account',
    [ExternalServiceKind.BITBUCKETSERVER]:
        'https://confluence.atlassian.com/bitbucketserver/ssh-user-keys-for-personal-use-776639793.html',
    [ExternalServiceKind.AWSCODECOMMIT]: 'unsupported',
    [ExternalServiceKind.BITBUCKETCLOUD]: 'unsupported',
    [ExternalServiceKind.GERRIT]: 'unsupported',
    [ExternalServiceKind.GITOLITE]: 'unsupported',
    [ExternalServiceKind.GOMODULES]: 'unsupported',
    [ExternalServiceKind.JVMPACKAGES]: 'unsupported',
    [ExternalServiceKind.NPMPACKAGES]: 'unsupported',
    [ExternalServiceKind.OTHER]: 'unsupported',
    [ExternalServiceKind.PERFORCE]: 'unsupported',
    [ExternalServiceKind.PAGURE]: 'unsupported',
    [ExternalServiceKind.PHABRICATOR]: 'unsupported',
}

export interface CodeHostSshPublicKeyProps {
    externalServiceKind: ExternalServiceKind
    sshPublicKey: string
    label?: string
    showInstructionsLink?: boolean
    showCopyButton?: boolean
}

export const CodeHostSshPublicKey: React.FunctionComponent<CodeHostSshPublicKeyProps> = ({
    externalServiceKind,
    sshPublicKey,
    showInstructionsLink = true,
    showCopyButton = true,
    label = 'Public SSH key',
}) => {
    const [copied, setCopied] = useState<boolean>(false)
    const onCopy = useCallback(() => {
        copy(sshPublicKey)
        setCopied(true)
    }, [sshPublicKey])
    return (
        <>
            <div className="d-flex justify-content-between align-items-end mb-2">
                <label htmlFor={LABEL_ID}>{label}</label>
                {showCopyButton && (
                    <Button onClick={onCopy} variant="secondary">
                        <Icon as={ContentCopyIcon} />
                        {copied ? 'Copied!' : 'Copy'}
                    </Button>
                )}
            </div>
            <TextArea
                id={LABEL_ID}
                className="text-monospace mb-3"
                rows={5}
                spellCheck="false"
                value={sshPublicKey}
                onChange={noop}
            />
            {showInstructionsLink && (
                <p>
                    <Link to={configInstructionLinks[externalServiceKind]} target="_blank" rel="noopener">
                        Configuration instructions
                    </Link>
                </p>
            )}
        </>
    )
}

const LABEL_ID = 'code-host-ssh-public-key-textarea'
