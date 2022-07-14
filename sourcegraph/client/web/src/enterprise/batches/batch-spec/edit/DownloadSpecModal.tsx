import React from 'react'

import { mdiClose } from '@mdi/js'
import { VisuallyHidden } from '@reach/visually-hidden'

import { CodeSnippet } from '@sourcegraph/branded/src/components/CodeSnippet'
import { Button, Link, Modal, H3, H4, Text, Icon } from '@sourcegraph/wildcard'

import { BatchSpecDownloadLink, getFileName } from '../../BatchSpec'

import styles from './DownloadSpecModal.module.scss'

// TODO: Several lines have been commented out to disable the "Don't show this again"
// functionality for "Download spec for src-cli" modal.
// See https://github.com/sourcegraph/sourcegraph/issues/37360.
// Uncomment all of these lines to restore it.

export interface DownloadSpecModalProps {
    name: string
    originalInput: string
    isLightTheme: boolean
    setIsDownloadSpecModalOpen: (condition: boolean) => void
    // setDownloadSpecModalDismissed: (condition: boolean) => void
}

export const DownloadSpecModal: React.FunctionComponent<React.PropsWithChildren<DownloadSpecModalProps>> = ({
    name,
    originalInput,
    isLightTheme,
    setIsDownloadSpecModalOpen,
    // setDownloadSpecModalDismissed,
}) => (
    <Modal
        onDismiss={() => {
            setIsDownloadSpecModalOpen(false)
        }}
        aria-labelledby={MODAL_LABEL_ID}
        className={styles.modal}
    >
        <div>
            <H3 id={MODAL_LABEL_ID}>Download spec for src-cli</H3>
            <Button
                className={styles.close}
                onClick={() => {
                    setIsDownloadSpecModalOpen(false)
                }}
            >
                <VisuallyHidden>Close</VisuallyHidden>
                <Icon className={styles.icon} svgPath={mdiClose} inline={false} aria-hidden={true} />
            </Button>
        </div>

        <div className={styles.container}>
            <div className={styles.left}>
                <Text>
                    Use the <Link to="/help/cli">Sourcegraph CLI (src)</Link>
                    to run this batch change locally.
                </Text>

                <CodeSnippet
                    code={`src batch preview -f ${getFileName(name)}`}
                    language="bash"
                    className={styles.codeSnippet}
                />

                <Text className="p-0 m-0">
                    Follow the URL printed in your terminal to see the preview and (when you're ready) create the batch
                    change.
                </Text>
            </div>
            <div className={styles.right}>
                <div>
                    <H4>About src-cli </H4>
                    <Text>
                        src cli is a command line interface to Sourcegraph. Its{' '}
                        <span className="text-monospace">batch</span> command allows to run batch specification files
                        using Docker.
                    </Text>
                    <Link to="https://docs.sourcegraph.com/cli">Download src-cli</Link>
                </div>
            </div>
        </div>
        <div className="d-flex justify-content-between">
            {/* <Button className="p-0" onClick={() => setDownloadSpecModalDismissed(true)} variant="link">
                Don't show this again
            </Button> */}
            <div className="ml-auto">
                <Button
                    className="mr-2"
                    outline={true}
                    variant="secondary"
                    onClick={() => {
                        setIsDownloadSpecModalOpen(false)
                    }}
                >
                    Cancel
                </Button>
                <BatchSpecDownloadLink
                    name={name}
                    originalInput={originalInput}
                    isLightTheme={isLightTheme}
                    asButton={false}
                >
                    <Button
                        variant="primary"
                        onClick={() => {
                            setIsDownloadSpecModalOpen(false)
                        }}
                    >
                        Download spec
                    </Button>
                </BatchSpecDownloadLink>
            </div>
        </div>
    </Modal>
)

const MODAL_LABEL_ID = 'download-spec-modal'
